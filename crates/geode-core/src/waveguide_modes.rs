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

use crate::complex_lanczos::SparseComplexShiftInvertLanczos;
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

/// Graded sibling of [`rect_tri_mesh`]: same `nx × ny` structured
/// two-triangles-per-quad triangulation, but the grid lines along `x` and
/// `y` are distributed per the supplied [`RadialGrading`] strategies instead
/// of uniformly. (The grading enum is reused; "radial" reads as "axial"
/// here — the same fraction generator applies to a 1-D span.)
///
/// `x_grading` distributes the `nx` columns across `[0, width]`;
/// `y_grading` the `ny` rows across `[0, height]`. The domain-boundary grid
/// lines (`0`, `width`, `height`) stay fixed, so the PEC wall masks
/// ([`rect_pec_interior_edges`] / [`rect_pec_interior_nodes`]) — which key on
/// geometry — work unchanged. [`RadialGrading::InterfaceClustered`] clusters
/// toward the far edge (`x = width` / `y = height`).
///
/// With both gradings [`RadialGrading::Uniform`] this reproduces
/// [`rect_tri_mesh`] **bit-for-bit**.
///
/// # Panics
///
/// Same `nx, ny ≥ 1` assertion as [`rect_tri_mesh`], plus the per-grading
/// parameter validity checks (see [`RadialGrading`]).
pub fn rect_tri_mesh_graded(
    nx: usize,
    ny: usize,
    width: f64,
    height: f64,
    x_grading: RadialGrading,
    y_grading: RadialGrading,
) -> TriMesh {
    assert!(nx >= 1 && ny >= 1, "rect_tri_mesh requires nx, ny ≥ 1");
    let npx = nx + 1;
    let npy = ny + 1;

    // Grid-line coordinates: index 0 is the origin edge (0.0), then the `nx`
    // (resp. `ny`) graded fractions scaled to the span. For Uniform we use
    // the *exact* original arithmetic (`i·(span/n)`) so the graded mesher
    // reproduces `rect_tri_mesh` bit-for-bit; graded axes scale the
    // generated fractions by the span.
    let axis = |n: usize, span: f64, grading: RadialGrading| -> Vec<f64> {
        if grading == RadialGrading::Uniform {
            let h = span / n as f64;
            return (0..=n).map(|i| i as f64 * h).collect();
        }
        let mut v = Vec::with_capacity(n + 1);
        v.push(0.0);
        for t in region_fractions(n, grading, InterfaceEdge::Outer) {
            v.push(span * t);
        }
        v
    };
    let xs = axis(nx, width, x_grading);
    let ys = axis(ny, height, y_grading);

    let node_idx = |i: usize, j: usize| -> u32 { (i + j * npx) as u32 };

    let mut nodes = Vec::with_capacity(npx * npy);
    for &y in &ys {
        for &x in &xs {
            nodes.push([x, y]);
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

/// Radial **grading strategy** for the concentric-ring disk meshers — how
/// the `n` ring radii inside a single region `[r_inner, r_outer]` are
/// distributed.
///
/// Grading redistributes the rings **within** a region; the region-boundary
/// rings (`r_inner`, `r_outer`) always stay fixed, so the conforming-ring
/// structure of [`disk_tri_mesh`] / [`disk_tri_mesh_pml`] is preserved (a
/// ring boundary still lands exactly on `core_radius`, `cladding_outer`, and
/// `outer_radius`) and the centroid-radius region tagging stays unambiguous
/// under any grading.
///
/// # Strategies
///
/// Let `n` be the number of radial subdivisions and let the (open-ended)
/// fractional positions `t_1 < t_2 < … < t_n = 1` map a region span onto
/// `r_k = r_inner + (r_outer − r_inner)·t_k`.
///
/// - [`RadialGrading::Uniform`] — `t_k = k/n`. The original behavior; the
///   meshers reproduce the un-graded output **bit-for-bit** with this.
/// - [`RadialGrading::Geometric`] — adjacent **steps** scale by a constant
///   `ratio`: `Δ_{k+1} = ratio·Δ_k`. `ratio > 1` clusters rings toward
///   `r_inner` (the inner edge); `ratio < 1` clusters toward `r_outer`.
/// - [`RadialGrading::Linear`] — step grows/shrinks **linearly**: the last
///   step is `ratio×` the first (`ratio > 1` ⇒ coarsen outward / cluster
///   inward; `ratio < 1` ⇒ cluster outward).
/// - [`RadialGrading::InterfaceClustered`] — densify toward the region edge
///   nearest the core–cladding interface `r = a`. `strength > 0`; larger ⇒
///   tighter clustering at that interface edge. (For the core region the
///   dense edge is `r_outer = a`; for the cladding/PML regions it is the
///   inner edge `r_inner = a`.)
///
/// Stronger grading produces more anisotropic (sliver) cells; the graded
/// meshers expose the worst triangle aspect ratio so callers can reject
/// pathological configs (see [`disk_tri_mesh_graded`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RadialGrading {
    /// Uniform radial step (the original, default behavior).
    #[default]
    Uniform,
    /// Geometric progression of the radial step by a constant `ratio`.
    /// `ratio > 1` clusters rings toward the inner edge; `ratio < 1`
    /// toward the outer edge. Must be finite and `> 0`; `1.0` ≡ uniform.
    Geometric {
        /// Multiplicative factor between adjacent radial steps.
        ratio: f64,
    },
    /// Linearly varying radial step: the last step is `ratio×` the first.
    /// `ratio > 1` clusters rings toward the inner edge; `ratio < 1`
    /// toward the outer edge. Must be finite and `> 0`; `1.0` ≡ uniform.
    Linear {
        /// Ratio of the last radial step to the first.
        ratio: f64,
    },
    /// Cluster rings toward the region edge nearest the core–cladding
    /// interface (`r = a`). `strength > 0`; larger ⇒ tighter clustering.
    /// `0.0` ≡ uniform.
    InterfaceClustered {
        /// Clustering strength toward the interface edge.
        strength: f64,
    },
}

/// Which edge of a region abuts the core–cladding interface (`r = a`), so
/// [`RadialGrading::InterfaceClustered`] knows which way to densify.
#[derive(Debug, Clone, Copy, PartialEq)]
enum InterfaceEdge {
    /// The region's outer edge (`r_outer`) is the interface (e.g. the core,
    /// whose outer edge is `r = a`).
    Outer,
    /// The region's inner edge (`r_inner`) is the interface (e.g. the
    /// cladding / PML annulus, whose inner edge is `r = a`).
    Inner,
}

/// Compute the `n` fractional ring positions `t_1 < … < t_n = 1` in `(0, 1]`
/// for one region under the given grading. `t_k` is then mapped to a radius
/// by `r_inner + (r_outer − r_inner)·t_k`.
///
/// `edge` selects the interface-adjacent edge for
/// [`RadialGrading::InterfaceClustered`] (ignored otherwise). The returned
/// vector has length `n`, is strictly increasing, and ends exactly at `1.0`.
///
/// For [`RadialGrading::Uniform`] the result is exactly `k/n` for
/// `k = 1..=n`, so the graded meshers reproduce the un-graded radii
/// bit-for-bit.
fn region_fractions(n: usize, grading: RadialGrading, edge: InterfaceEdge) -> Vec<f64> {
    debug_assert!(n >= 1);
    match grading {
        RadialGrading::Uniform => (1..=n).map(|k| k as f64 / n as f64).collect(),
        RadialGrading::Geometric { ratio } => {
            assert!(
                ratio.is_finite() && ratio > 0.0,
                "RadialGrading::Geometric ratio must be finite and > 0 (got {ratio})"
            );
            if ratio == 1.0 {
                return (1..=n).map(|k| k as f64 / n as f64).collect();
            }
            // Steps Δ_k = Δ_1·ratio^(k-1), k = 1..=n. Σ Δ_k = span = 1,
            // so Δ_1 = (1 − ratio) / (1 − ratio^n). Cumulative sums give t_k.
            let mut steps = Vec::with_capacity(n);
            let mut s = 1.0_f64;
            for _ in 0..n {
                steps.push(s);
                s *= ratio;
            }
            let total: f64 = steps.iter().sum();
            let mut t = Vec::with_capacity(n);
            let mut acc = 0.0;
            for (i, st) in steps.iter().enumerate() {
                acc += st / total;
                // Pin the last fraction to exactly 1.0 (no FP drift on the
                // region boundary, preserving conformity).
                t.push(if i + 1 == n { 1.0 } else { acc });
            }
            t
        }
        RadialGrading::Linear { ratio } => {
            assert!(
                ratio.is_finite() && ratio > 0.0,
                "RadialGrading::Linear ratio must be finite and > 0 (got {ratio})"
            );
            if ratio == 1.0 {
                return (1..=n).map(|k| k as f64 / n as f64).collect();
            }
            // Step k (k = 0..n-1) interpolates linearly from 1 to `ratio`:
            //   Δ_k = 1 + (ratio − 1)·k/(n−1)   (for n ≥ 2; n == 1 ⇒ single
            // step). Normalize the cumulative sum to land on 1.0.
            let mut steps = Vec::with_capacity(n);
            for k in 0..n {
                let frac = if n == 1 {
                    0.0
                } else {
                    k as f64 / (n - 1) as f64
                };
                steps.push(1.0 + (ratio - 1.0) * frac);
            }
            let total: f64 = steps.iter().sum();
            let mut t = Vec::with_capacity(n);
            let mut acc = 0.0;
            for (i, st) in steps.iter().enumerate() {
                acc += st / total;
                t.push(if i + 1 == n { 1.0 } else { acc });
            }
            t
        }
        RadialGrading::InterfaceClustered { strength } => {
            assert!(
                strength.is_finite() && strength >= 0.0,
                "RadialGrading::InterfaceClustered strength must be finite and ≥ 0 (got {strength})"
            );
            if strength == 0.0 {
                return (1..=n).map(|k| k as f64 / n as f64).collect();
            }
            // Map uniform fractions u_k = k/n through a stretching function
            // that clusters samples toward one end. We use a power law on the
            // *gap* from the dense edge: dense at u = 0 ⇒ x = u^(1+strength);
            // dense at u = 1 ⇒ x = 1 − (1 − u)^(1+strength). The `edge`
            // selects which physical end (r_inner / r_outer) is dense; since
            // t maps r_inner→0 and r_outer→1, Outer-dense clusters near t = 1
            // and Inner-dense clusters near t = 0.
            let p = 1.0 + strength;
            let mut t = Vec::with_capacity(n);
            for k in 1..=n {
                let u = k as f64 / n as f64;
                let x = match edge {
                    // Dense toward r_outer (t = 1).
                    InterfaceEdge::Outer => 1.0 - (1.0 - u).powf(p),
                    // Dense toward r_inner (t = 0).
                    InterfaceEdge::Inner => u.powf(p),
                };
                t.push(if k == n { 1.0 } else { x });
            }
            t
        }
    }
}

/// Append the `n` ring radii for the **core band** `[0, core_radius]` to
/// `ring_r`, graded per `grading`. The core band's interface-adjacent edge
/// is its outer edge (`r = core_radius`).
///
/// For [`RadialGrading::Uniform`] this uses the **exact** original
/// arithmetic `core_radius·k / n` (left-associative: `(core_radius·k)/n`) so
/// the un-graded meshers reproduce bit-for-bit.
fn push_core_band(ring_r: &mut Vec<f64>, core_radius: f64, n: usize, grading: RadialGrading) {
    if grading == RadialGrading::Uniform {
        for k in 1..=n {
            ring_r.push(core_radius * k as f64 / n as f64);
        }
        return;
    }
    for t in region_fractions(n, grading, InterfaceEdge::Outer) {
        ring_r.push(core_radius * t);
    }
}

/// Append the `n` ring radii for an **outer band** `[r_inner, r_outer]` to
/// `ring_r`, graded per `grading`. `edge` selects the interface-adjacent edge
/// for [`RadialGrading::InterfaceClustered`].
///
/// For [`RadialGrading::Uniform`] this uses the **exact** original arithmetic
/// `r_inner + (r_outer − r_inner)·(k/n)` so the un-graded meshers reproduce
/// bit-for-bit.
fn push_outer_band(
    ring_r: &mut Vec<f64>,
    r_inner: f64,
    r_outer: f64,
    n: usize,
    grading: RadialGrading,
    edge: InterfaceEdge,
) {
    if grading == RadialGrading::Uniform {
        for k in 1..=n {
            let t = k as f64 / n as f64;
            ring_r.push(r_inner + (r_outer - r_inner) * t);
        }
        return;
    }
    for t in region_fractions(n, grading, edge) {
        ring_r.push(r_inner + (r_outer - r_inner) * t);
    }
}

/// Worst triangle **aspect ratio** in a mesh: `longest_edge / (2·inradius)`
/// (≈ 1 for an equilateral triangle, large for slivers). This mirrors the
/// quality metric the `disk_tri_mesh_*` unit tests assert, exposed so
/// callers of the graded meshers can detect grading-induced slivers.
pub fn worst_aspect_ratio(mesh: &TriMesh) -> f64 {
    let mut worst = 0.0_f64;
    for t in &mesh.tris {
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
        // Signed area via the shoelace formula (positive for CCW).
        let area = 0.5
            * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]))
                .abs();
        if area <= 0.0 {
            return f64::INFINITY;
        }
        let s = 0.5 * (l01 + l12 + l20);
        let inradius = area / s;
        worst = worst.max(longest / (2.0 * inradius));
    }
    worst
}

/// Default upper bound on the worst triangle aspect ratio for the *checked*
/// graded meshers ([`disk_tri_mesh_graded_checked`] /
/// [`disk_tri_mesh_pml_graded_checked`]). Strong grading produces slivers;
/// configs whose worst aspect ratio exceeds this are rejected. The value is
/// generous (the un-graded balanced-knob meshes sit well under ~7) so only
/// genuinely pathological grading trips it.
pub const ASPECT_RATIO_SLIVER_BOUND: f64 = 25.0;

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
    // Un-graded behavior is exactly the `Uniform` grading; delegate so there
    // is one triangulation path. `region_fractions(Uniform)` returns `k/n`
    // exactly, so the ring radii — and hence the whole mesh — are bit-for-bit
    // identical to the original construction.
    disk_tri_mesh_graded(
        core_radius,
        outer_radius,
        n_radial,
        n_angular,
        RadialGrading::Uniform,
        RadialGrading::Uniform,
    )
}

/// Graded sibling of [`disk_tri_mesh`]: same conforming concentric-ring
/// triangulation, but the `n_radial` rings within the **core** and within
/// the **cladding** are distributed per the supplied [`RadialGrading`]
/// strategies instead of uniformly.
///
/// `core_grading` controls the rings in `0 ≤ r ≤ core_radius`;
/// `cladding_grading` controls `core_radius ≤ r ≤ outer_radius`. The
/// region-boundary rings (`core_radius`, `outer_radius`) stay fixed, so the
/// core circle is still conformed and the centroid-radius region tagging is
/// still unambiguous.
///
/// With both gradings [`RadialGrading::Uniform`] this reproduces
/// [`disk_tri_mesh`] **bit-for-bit** (same nodes, triangles, and tags).
///
/// Returns `(mesh, region_tags)` exactly as [`disk_tri_mesh`]. For a config
/// that also rejects sliver-producing grading, see
/// [`disk_tri_mesh_graded_checked`].
///
/// # Panics
///
/// Same radius / knob assertions as [`disk_tri_mesh`], plus the per-grading
/// parameter validity checks (see [`RadialGrading`]).
pub fn disk_tri_mesh_graded(
    core_radius: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
    core_grading: RadialGrading,
    cladding_grading: RadialGrading,
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
    // Grading redistributes the rings *within* each band; the band-boundary
    // rings stay fixed. The core's interface-adjacent edge is its outer edge
    // (r = core_radius); the cladding's is its inner edge (r = core_radius).
    let n_rings = 2 * n_radial; // number of annular layers (rings of cells)
    let mut ring_r = Vec::with_capacity(n_rings + 1);
    ring_r.push(0.0);
    push_core_band(&mut ring_r, core_radius, n_radial, core_grading);
    push_outer_band(
        &mut ring_r,
        core_radius,
        outer_radius,
        n_radial,
        cladding_grading,
        InterfaceEdge::Inner,
    );
    debug_assert_eq!(ring_r.len(), n_rings + 1);

    build_disk_mesh(&ring_r, n_angular, &|r| {
        if r < core_radius {
            REGION_CORE
        } else {
            REGION_CLADDING
        }
    })
}

/// [`disk_tri_mesh_graded`] with a **mesh-quality guard**: if the worst
/// triangle aspect ratio exceeds `aspect_bound` (a sane default is
/// [`ASPECT_RATIO_SLIVER_BOUND`]), the grading is rejected with an `Err`
/// describing the offending aspect ratio, rather than silently returning a
/// sliver-laden mesh. On success returns `Ok((mesh, region_tags))`.
#[allow(clippy::too_many_arguments)]
pub fn disk_tri_mesh_graded_checked(
    core_radius: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
    core_grading: RadialGrading,
    cladding_grading: RadialGrading,
    aspect_bound: f64,
) -> Result<(TriMesh, Vec<i32>), String> {
    let (mesh, tags) = disk_tri_mesh_graded(
        core_radius,
        outer_radius,
        n_radial,
        n_angular,
        core_grading,
        cladding_grading,
    );
    let worst = worst_aspect_ratio(&mesh);
    if worst > aspect_bound {
        return Err(format!(
            "disk_tri_mesh_graded_checked: worst aspect ratio {worst:.3} exceeds bound \
             {aspect_bound:.3} — grading too strong (core={core_grading:?}, \
             cladding={cladding_grading:?})"
        ));
    }
    Ok((mesh, tags))
}

/// Shared concentric-ring triangulation from a precomputed strictly
/// increasing `ring_r` (with `ring_r[0] == 0` the center). Builds the central
/// fan + annular quads exactly as the original meshers and tags each triangle
/// by its centroid radius via `tag_of`. Used by every disk mesher so the
/// triangulation lives in one place.
fn build_disk_mesh(
    ring_r: &[f64],
    n_angular: usize,
    tag_of: &dyn Fn(f64) -> i32,
) -> (TriMesh, Vec<i32>) {
    let n_rings = ring_r.len() - 1;
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

    // Per-triangle region tags by centroid radius. Because ring boundaries
    // sit exactly on the region radii, every triangle is wholly inside one
    // region and the centroid test is unambiguous.
    let region_tags: Vec<i32> = tris
        .iter()
        .map(|t| {
            let xc =
                (nodes[t[0] as usize][0] + nodes[t[1] as usize][0] + nodes[t[2] as usize][0]) / 3.0;
            let yc =
                (nodes[t[0] as usize][1] + nodes[t[1] as usize][1] + nodes[t[2] as usize][1]) / 3.0;
            tag_of((xc * xc + yc * yc).sqrt())
        })
        .collect();

    (TriMesh { nodes, tris }, region_tags)
}

/// Per-triangle region tag for a core triangle in [`disk_tri_mesh`] /
/// [`disk_tri_mesh_pml`] (centroid radius `< core_radius`).
pub const REGION_CORE: i32 = 1;
/// Per-triangle region tag for a cladding triangle (`core_radius ≤ r`,
/// and `r < R_pml_inner` for the PML variant).
pub const REGION_CLADDING: i32 = 0;
/// Per-triangle region tag for a PML-annulus triangle in
/// [`disk_tri_mesh_pml`] (centroid radius `≥ R_pml_inner`).
pub const REGION_PML: i32 = 2;

/// Concentric-ring disk mesh with a **three-region** tagging — core,
/// cladding, and an outermost **PML annulus** — for the 2D UPML modal
/// solver (Epic #303 PML-A, issue #331).
///
/// This is the PML-tagged sibling of [`disk_tri_mesh`]. The radial layout
/// reuses the same conforming concentric-ring construction, but adds a
/// third radial band so that **ring boundaries land exactly on both**
/// `core_radius` **and** `r_pml_inner` (= `cladding_outer`). Every triangle
/// is therefore wholly inside one region by the unambiguous centroid test
/// (the same robustness guarantee [`disk_tri_mesh`] gives for the
/// core/cladding split):
///
/// ```text
///   centroid r < core_radius          → REGION_CORE     (tag 1)
///   core_radius ≤ centroid r < r_pml  → REGION_CLADDING (tag 0)
///   r_pml ≤ centroid r                → REGION_PML       (tag 2)
/// ```
///
/// where `r_pml_inner = cladding_outer` and the PML annulus occupies
/// `cladding_outer ≤ r ≤ outer_radius` (thickness `outer_radius −
/// cladding_outer`). Each region gets `n_radial` radial subdivisions, so a
/// ring boundary sits exactly on `core_radius`, on `cladding_outer`, and on
/// `outer_radius`.
///
/// # Outer boundary condition
///
/// The very outer edge (`r = outer_radius`) keeps a **thin PEC backing**:
/// the existing [`disk_pec_interior_dofs2`] / [`disk_pec_interior_edges`]
/// masks (which key on `outer_radius`) are reused unchanged as the PML
/// termination. This is the standard UPML setup — the absorbing layer
/// attenuates the field before it reaches the PEC wall, so the wall sees a
/// negligible round-trip reflection and no box / cladding-resonance modes
/// form in the guided window. With `sigma_0 = 0` the layer is transparent
/// and the mesh degenerates (physically) to a plain PEC-walled disk.
///
/// # Returns
///
/// `(mesh, region_tags)` where `region_tags[t] ∈ {REGION_CORE,
/// REGION_CLADDING, REGION_PML}`. Feed the core/cladding tags into
/// [`epsilon_r_from_region_tags`] for the per-triangle `ε_r`; feed the full
/// tag vector into [`assemble_2d_nedelec2_pml_sparse_interior`] to flag the
/// PML-stretched triangles.
///
/// # Panics
///
/// Panics unless `0 < core_radius < cladding_outer < outer_radius`,
/// `n_radial ≥ 1`, and `n_angular ≥ 3`.
pub fn disk_tri_mesh_pml(
    core_radius: f64,
    cladding_outer: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
) -> (TriMesh, Vec<i32>) {
    // Un-graded behavior is exactly `Uniform` grading in all three bands;
    // delegate so the triangulation lives in one place. Bit-for-bit identical.
    disk_tri_mesh_pml_graded(
        core_radius,
        cladding_outer,
        outer_radius,
        n_radial,
        n_angular,
        RadialGrading::Uniform,
        RadialGrading::Uniform,
        RadialGrading::Uniform,
    )
}

/// Graded sibling of [`disk_tri_mesh_pml`]: same conforming three-band
/// concentric-ring triangulation (core / cladding / PML annulus), but the
/// `n_radial` rings within each band are distributed per the supplied
/// [`RadialGrading`] strategies instead of uniformly.
///
/// `core_grading` controls `0 ≤ r ≤ core_radius`; `cladding_grading`
/// controls `core_radius ≤ r ≤ cladding_outer`; `pml_grading` controls
/// `cladding_outer ≤ r ≤ outer_radius`. The band-boundary rings
/// (`core_radius`, `cladding_outer`, `outer_radius`) stay fixed, so all three
/// region interfaces are still conformed and the centroid-radius region
/// tagging is still unambiguous.
///
/// With all three gradings [`RadialGrading::Uniform`] this reproduces
/// [`disk_tri_mesh_pml`] **bit-for-bit**.
///
/// This is the PML mesher a downstream graded-fiber experiment needs.
///
/// Returns `(mesh, region_tags)` exactly as [`disk_tri_mesh_pml`] (tags in
/// `{REGION_CORE, REGION_CLADDING, REGION_PML}`). For a config that also
/// rejects sliver-producing grading, see [`disk_tri_mesh_pml_graded_checked`].
///
/// # Panics
///
/// Same radius / knob assertions as [`disk_tri_mesh_pml`], plus the
/// per-grading parameter validity checks (see [`RadialGrading`]).
#[allow(clippy::too_many_arguments)]
pub fn disk_tri_mesh_pml_graded(
    core_radius: f64,
    cladding_outer: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
    core_grading: RadialGrading,
    cladding_grading: RadialGrading,
    pml_grading: RadialGrading,
) -> (TriMesh, Vec<i32>) {
    assert!(
        core_radius.is_finite() && cladding_outer.is_finite() && outer_radius.is_finite(),
        "disk_tri_mesh_pml radii must be finite"
    );
    assert!(
        0.0 < core_radius && core_radius < cladding_outer && cladding_outer < outer_radius,
        "disk_tri_mesh_pml requires 0 < core_radius ({core_radius}) < cladding_outer \
         ({cladding_outer}) < outer_radius ({outer_radius})"
    );
    assert!(n_radial >= 1, "disk_tri_mesh_pml requires n_radial ≥ 1");
    assert!(n_angular >= 3, "disk_tri_mesh_pml requires n_angular ≥ 3");

    // Three radial bands, each with `n_radial` subdivisions; ring boundaries
    // land exactly on core_radius, cladding_outer (= r_pml_inner), and
    // outer_radius. Total rings of cells = 3·n_radial. The core's
    // interface-adjacent edge is its outer edge (r = a); the cladding's is
    // its inner edge (r = a). The PML band's interface-clustering densifies
    // toward its inner edge (the cladding boundary).
    let n_rings = 3 * n_radial;
    let mut ring_r = Vec::with_capacity(n_rings + 1);
    ring_r.push(0.0);
    push_core_band(&mut ring_r, core_radius, n_radial, core_grading);
    push_outer_band(
        &mut ring_r,
        core_radius,
        cladding_outer,
        n_radial,
        cladding_grading,
        InterfaceEdge::Inner,
    );
    push_outer_band(
        &mut ring_r,
        cladding_outer,
        outer_radius,
        n_radial,
        pml_grading,
        InterfaceEdge::Inner,
    );
    debug_assert_eq!(ring_r.len(), n_rings + 1);

    build_disk_mesh(&ring_r, n_angular, &|r| {
        if r < core_radius {
            REGION_CORE
        } else if r < cladding_outer {
            REGION_CLADDING
        } else {
            REGION_PML
        }
    })
}

/// [`disk_tri_mesh_pml_graded`] with a **mesh-quality guard**: if the worst
/// triangle aspect ratio exceeds `aspect_bound` (a sane default is
/// [`ASPECT_RATIO_SLIVER_BOUND`]), the grading is rejected with an `Err`
/// rather than silently returning a sliver-laden mesh.
#[allow(clippy::too_many_arguments)]
pub fn disk_tri_mesh_pml_graded_checked(
    core_radius: f64,
    cladding_outer: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
    core_grading: RadialGrading,
    cladding_grading: RadialGrading,
    pml_grading: RadialGrading,
    aspect_bound: f64,
) -> Result<(TriMesh, Vec<i32>), String> {
    let (mesh, tags) = disk_tri_mesh_pml_graded(
        core_radius,
        cladding_outer,
        outer_radius,
        n_radial,
        n_angular,
        core_grading,
        cladding_grading,
        pml_grading,
    );
    let worst = worst_aspect_ratio(&mesh);
    if worst > aspect_bound {
        return Err(format!(
            "disk_tri_mesh_pml_graded_checked: worst aspect ratio {worst:.3} exceeds bound \
             {aspect_bound:.3} — grading too strong (core={core_grading:?}, \
             cladding={cladding_grading:?}, pml={pml_grading:?})"
        ));
    }
    Ok((mesh, tags))
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

/// 6-point degree-4 symmetric Gauss quadrature on the reference triangle,
/// as `(λ₀, λ₁, λ₂, weight)` rows in **barycentric** coordinates.
///
/// The weights are normalised to sum to `1` (they integrate against the
/// element area, i.e. `∫_T f dA ≈ |T| · Σ w_q f(λ_q)`). This is the
/// classic Strang–Fix / Dunavant degree-4 rule with two orbits of three
/// permutation points:
///
/// ```text
///   orbit A:  (α, β, β) and perms,   α = 0.108_103_018_168_070,
///                                    β = 0.445_948_490_915_965,
///             weight = 0.223_381_589_678_011   (×3)
///   orbit B:  (γ, δ, δ) and perms,   γ = 0.816_847_572_980_459,
///                                    δ = 0.091_576_213_509_771,
///             weight = 0.109_951_743_655_322   (×3)
/// ```
///
/// It integrates any bivariate polynomial of total degree ≤ 4 exactly,
/// which covers both the curl-curl integrand (degree ≤ 2) and the
/// `N_i·N_j` mass integrand (degree ≤ 4) of the p=2 element on an affine
/// (constant-Jacobian) triangle.
pub const TRI_QUAD_DEG4: [[f64; 4]; 6] = {
    const A: f64 = 0.108_103_018_168_070;
    const B: f64 = 0.445_948_490_915_965;
    const WA: f64 = 0.223_381_589_678_011;
    const G: f64 = 0.816_847_572_980_459;
    const D: f64 = 0.091_576_213_509_771;
    const WB: f64 = 0.109_951_743_655_322;
    [
        [A, B, B, WA],
        [B, A, B, WA],
        [B, B, A, WA],
        [G, D, D, WB],
        [D, G, D, WB],
        [D, D, G, WB],
    ]
};

/// Local p=2 Nédélec-first-kind (curl-conforming) element kernel for an
/// affine triangle, built as a **hierarchical extension** of the
/// first-order Whitney basis ([`tri_nedelec_local`]).
///
/// Returns `(K, M, signed_area)` where `K` (8×8) is the curl-curl
/// stiffness `∫ (∇×N_i)(∇×N_j) dA`, `M` (8×8) is the mass
/// `∫ N_i·N_j dA`, and the signed area matches `tri_nedelec_local`.
///
/// # DOF layout (8 = 6 edge + 2 interior)
///
/// Edges follow [`TRI_LOCAL_EDGES`] order `e₀=(0,1), e₁=(0,2), e₂=(1,2)`.
/// For each edge `(a, b)` there are two hierarchical functions:
///
/// ```text
///   DOF 2k    Whitney (odd):     W = λ_a ∇λ_b − λ_b ∇λ_a
///   DOF 2k+1  gradient (even):   Q = λ_a ∇λ_b + λ_b ∇λ_a = ∇(λ_a λ_b)
/// ```
///
/// so the local DOFs are `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]`.
///
/// The **first edge DOF per edge is exactly the Whitney function** of
/// `tri_nedelec_local`, so the 3×3 sub-block of `K`/`M` over indices
/// `{0, 2, 4}` is bit-for-bit the first-order kernel (a strict,
/// test-verified subset). The Whitney function flips sign with global
/// edge orientation; the gradient function `Q = ∇(λ_a λ_b)` is symmetric
/// under `a ↔ b` and so is orientation-independent. Curl `∇×Q = 0`.
///
/// The two **interior (face) bubbles** are orientation-independent
/// (defined per-triangle by vertex index):
///
/// ```text
///   I₀ = λ₂ (λ₀ ∇λ₁ − λ₁ ∇λ₀) = λ₂ W₀
///   I₁ = λ₀ (λ₁ ∇λ₂ − λ₂ ∇λ₁) = λ₀ W₂
/// ```
///
/// These two complete the Nédélec-1st-kind order-2 space (dimension 8).
///
/// # Curls
///
/// Every basis function is a sum of terms `f · ∇λ_p` with `f` a
/// barycentric polynomial and `∇λ_p` constant. The scalar curl of such a
/// term is `(∇f × ∇λ_p)_z`, and `∇(λ_q) = ∇λ_q` is constant, so
/// `∇(λ_q λ_r) = λ_q ∇λ_r + λ_r ∇λ_q` and every curl is an exact linear
/// (degree ≤ 1) field evaluated at the quadrature points.
pub fn tri_nedelec2_local(coords: &[[f64; 2]; 3]) -> ([[f64; 8]; 8], [[f64; 8]; 8], f64) {
    // Affine Jacobian setup — identical to tri_nedelec_local.
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let abs_det = det.abs();
    let area_abs = 0.5 * abs_det;

    // Constant barycentric gradients g_p = ∇λ_p.
    let g = [
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

    // 2-D scalar cross product (z-component): used for curls of f·∇λ_p.
    let cross = |u: [f64; 2], v: [f64; 2]| -> f64 { u[0] * v[1] - u[1] * v[0] };

    // Evaluate the 8 vector basis functions and their scalar curls at a
    // barycentric point `lam = (λ₀, λ₁, λ₂)`. Returns (values[8], curls[8]).
    let eval = |lam: [f64; 3]| -> ([[f64; 2]; 8], [f64; 8]) {
        let (l0, l1, l2) = (lam[0], lam[1], lam[2]);

        // Whitney edge functions W_(a,b) = λ_a g_b − λ_b g_a (constant curl
        // = 2 (g_a × g_b)_z), in TRI_LOCAL_EDGES order.
        let whitney = |a: usize, b: usize, la: f64, lb: f64| -> ([f64; 2], f64) {
            let val = [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]];
            let curl = 2.0 * cross(g[a], g[b]);
            (val, curl)
        };
        let (w0, cw0) = whitney(0, 1, l0, l1);
        let (w1, cw1) = whitney(0, 2, l0, l2);
        let (w2, cw2) = whitney(1, 2, l1, l2);

        // Gradient edge functions Q_(a,b) = λ_a g_b + λ_b g_a = ∇(λ_a λ_b),
        // curl ≡ 0.
        let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
            [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
        };
        let q0 = qgrad(0, 1, l0, l1);
        let q1 = qgrad(0, 2, l0, l2);
        let q2 = qgrad(1, 2, l1, l2);

        // Interior bubbles I = λ_c · W_(a,b).
        //   curl(λ_c W) = (∇λ_c × W)_z + λ_c (∇×W)
        // with ∇×W constant = 2(g_a × g_b)_z.
        let bubble = |w: [f64; 2], cw: f64, c: usize, lc: f64| -> ([f64; 2], f64) {
            let val = [lc * w[0], lc * w[1]];
            let curl = cross(g[c], w) + lc * cw;
            (val, curl)
        };
        // I₀ = λ₂ W₀, I₁ = λ₀ W₂.
        let (i0, ci0) = bubble(w0, cw0, 2, l2);
        let (i1, ci1) = bubble(w2, cw2, 0, l0);

        let vals = [w0, q0, w1, q1, w2, q2, i0, i1];
        let curls = [cw0, 0.0, cw1, 0.0, cw2, 0.0, ci0, ci1];
        (vals, curls)
    };

    let mut k_local = [[0.0_f64; 8]; 8];
    let mut m_local = [[0.0_f64; 8]; 8];

    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area_abs; // physical-area quadrature weight
        let (vals, curls) = eval(lam);
        for i in 0..8 {
            for j in 0..8 {
                k_local[i][j] += w * curls[i] * curls[j];
                m_local[i][j] += w * (vals[i][0] * vals[j][0] + vals[i][1] * vals[j][1]);
            }
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

/// Global degree-of-freedom count for the p=2 Nédélec system on `mesh`:
/// `2·n_edges + 2·n_tris`.
///
/// Two contiguous edge DOFs per global edge (the Whitney function `W`
/// followed by the gradient function `Q = ∇(λ_a λ_b)`), then two interior
/// (face) bubble DOFs per triangle appended **after all edge DOFs**. See
/// [`assemble_2d_nedelec2_with_epsilon`] for the full numbering scheme.
pub fn n_dof_2d_nedelec2(mesh: &TriMesh) -> usize {
    2 * mesh.edges().len() + 2 * mesh.n_tris()
}

/// Per-DOF orientation-sign of the 8 local p=2 basis functions, in the
/// local DOF order `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]`.
///
/// `flips[i] == true` means local DOF `i` **flips sign** with the global
/// orientation of its underlying global edge (so the scatter must multiply
/// by that edge's `tri_edges` sign); `false` means the function is
/// orientation-independent (scatter sign `+1`).
///
/// Only the three **Whitney** edge functions (the *odd* hierarchical edge
/// functions, local DOFs `0, 2, 4`) flip — exactly as the first-order
/// Whitney DOF does in [`TriMesh::tri_edges`]. The three **gradient** edge
/// functions `Q = ∇(λ_a λ_b)` (local DOFs `1, 3, 5`) are *even* (symmetric
/// under `a ↔ b`), so they are orientation-independent; the two interior
/// bubbles (local DOFs `6, 7`) are per-triangle and never shared, so they
/// are orientation-independent as well.
///
/// This sign vector is the single most error-prone piece of the p=2
/// assembly: a wrong sign on a Whitney function would silently corrupt the
/// assembled operator across every shared edge.
pub const TRI_NEDELEC2_DOF_FLIPS: [bool; 8] = [true, false, true, false, true, false, false, false];

/// Map a triangle's 8 local p=2 DOFs to their `(global_index, sign)`
/// pairs, in the local DOF order `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]`.
///
/// `tri_edges_row` is one row of [`TriMesh::tri_edges`] (the three
/// `(global_edge_index, orientation_sign)` pairs for this triangle's local
/// edges, in [`TRI_LOCAL_EDGES`] order). `tri_index` is the triangle's
/// index in `mesh.tris`, and `n_edges` is `mesh.edges().len()`.
///
/// Global numbering:
/// - edge `e` owns DOFs `2e` (Whitney `W`) and `2e+1` (gradient `Q`);
/// - triangle `t` owns interior DOFs `2·n_edges + 2t` (`I₀`) and
///   `2·n_edges + 2t + 1` (`I₁`).
///
/// Signs come from [`TRI_NEDELEC2_DOF_FLIPS`]: the Whitney DOFs carry their
/// edge's orientation sign; everything else carries `+1`.
fn tri_nedelec2_dofs(
    tri_edges_row: &[(u32, i8); 3],
    tri_index: usize,
    n_edges: usize,
) -> [(usize, f64); 8] {
    let mut out = [(0usize, 1.0f64); 8];
    // Six edge DOFs: two per local edge (Whitney then gradient). The per-DOF
    // orientation rule is read from [`TRI_NEDELEC2_DOF_FLIPS`] (the single
    // source of truth): a DOF whose `flips` entry is `true` carries its
    // edge's orientation sign; otherwise it carries `+1`.
    for (k, &(gedge, esign)) in tri_edges_row.iter().enumerate() {
        let base = 2 * gedge as usize;
        // Local DOF 2k = Whitney (odd → flips with edge orientation).
        let w_sign = if TRI_NEDELEC2_DOF_FLIPS[2 * k] {
            esign as f64
        } else {
            1.0
        };
        out[2 * k] = (base, w_sign);
        // Local DOF 2k+1 = gradient Q (even → orientation-independent).
        let q_sign = if TRI_NEDELEC2_DOF_FLIPS[2 * k + 1] {
            esign as f64
        } else {
            1.0
        };
        out[2 * k + 1] = (base + 1, q_sign);
    }
    // Two interior bubble DOFs, appended after all edge DOFs. The interior
    // bubbles (local DOFs 6, 7) are per-triangle and never shared, so
    // [`TRI_NEDELEC2_DOF_FLIPS`] marks them orientation-independent (`+1`).
    debug_assert!(!TRI_NEDELEC2_DOF_FLIPS[6] && !TRI_NEDELEC2_DOF_FLIPS[7]);
    let interior_base = 2 * n_edges + 2 * tri_index;
    out[6] = (interior_base, 1.0);
    out[7] = (interior_base + 1, 1.0);
    out
}

/// Assemble the dense global p=2 Nédélec curl-curl stiffness `K` and
/// **ε-weighted** mass `M` for a 2-D triangle mesh.
///
/// This is the higher-order (Epic #318 Phase 2.5B) analogue of
/// [`assemble_2d_nedelec_with_epsilon`]: it scatters the 8×8 local blocks
/// from [`tri_nedelec2_local`] into an `n_dof × n_dof` global system with
/// `n_dof = 2·n_edges + 2·n_tris` ([`n_dof_2d_nedelec2`]).
///
/// ## DOF numbering
///
/// - **Edge DOFs** reuse [`TriMesh::edges`] ordering: global edge `e` owns
///   two contiguous DOFs, `2e` (the Whitney function `W`, the strict p=1
///   subset) and `2e+1` (the gradient function `Q = ∇(λ_a λ_b)`).
/// - **Interior DOFs** are appended after **all** edge DOFs: triangle `t`
///   owns `2·n_edges + 2t` (`I₀`) and `2·n_edges + 2t + 1` (`I₁`).
///
/// ## Per-DOF orientation signs
///
/// The scatter applies a per-DOF sign (`sign_i · sign_j` on entry `(i, j)`)
/// taken from [`TRI_NEDELEC2_DOF_FLIPS`] via [`tri_nedelec2_dofs`]: the
/// three Whitney edge functions are *odd* and flip with the global edge
/// orientation (exactly like the first-order Whitney DOF); the three
/// gradient edge functions `Q` are *even* (symmetric under `a ↔ b`) and the
/// two interior bubbles are per-triangle, so all five are
/// orientation-independent. Getting the Whitney signs right is the key
/// correctness guard — a wrong sign would silently corrupt the operator at
/// every shared edge.
///
/// ## Where ε enters
///
/// Exactly as in [`assemble_2d_nedelec_with_epsilon`]: the per-triangle
/// scalar `ε_r` multiplies only the **mass** block `∫ ε_r N_i·N_j` (the
/// material metric of `E`); the curl-curl **stiffness** `K` carries
/// `1/μ_r = 1` and stays ε-independent.
///
/// ## p=1 subset
///
/// Restricting the returned `K`/`M` to the Whitney DOFs (global indices
/// `{2·0, 2·1, …}` — i.e. the even edge DOFs) reproduces
/// [`assemble_2d_nedelec_with_epsilon`] to floating-point tolerance,
/// because local DOFs `0, 2, 4` are exactly the first-order Whitney
/// functions and carry the same orientation signs.
///
/// Returns `(K, M)` of size `[n_dof, n_dof]`.
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()`.
pub fn assemble_2d_nedelec2_with_epsilon(mesh: &TriMesh, eps_r: &[f64]) -> (Mat<f64>, Mat<f64>) {
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
    let n_dof = 2 * n_edges + 2 * mesh.n_tris();

    let mut k = Mat::<f64>::zeros(n_dof, n_dof);
    let mut m = Mat::<f64>::zeros(n_dof, n_dof);

    for (tri_index, ((tri, row), &eps)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(eps_r.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        assert!(
            signed_area > 0.0,
            "rect_tri_mesh / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        for i in 0..8 {
            let (gi, si) = dofs[i];
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let s = si * sj;
                // K (curl-curl) is ε-independent for μ_r = 1.
                k[(gi, gj)] += s * k_local[i][j];
                // ε weights the mass term ∫ ε N_i·N_j per element.
                m[(gi, gj)] += s * eps * m_local[i][j];
            }
        }
    }

    (k, m)
}

/// Interior-restricted sparse Nédélec operators for the dielectric / modal
/// eigenproblem, assembled **directly** as `faer` `SparseColMat` from the
/// per-element local blocks — never materializing the dense `N×N` `Mat`.
///
/// This is the sparse analogue of building
/// [`assemble_2d_nedelec_with_epsilon`] /
/// [`assemble_2d_nedelec2_with_epsilon`] and then [`apply_pec_2d`]-restricting
/// to interior DOFs, but it folds assembly + interior restriction + the dense
/// → sparse round-trip into one pass.
///
/// The returned matrices are **interior-restricted** (size `dim × dim` where
/// `dim` is the number of `true` entries in `interior_mask`), exactly what the
/// shift-invert Lanczos eigensolve consumes. Their nonzeros equal, entry for
/// entry, the dense path's `apply_pec_2d(&assemble_2d_nedelec*…)` output:
/// `faer`'s `try_new_from_triplets` sums duplicate `(row, col)` triplets, which
/// is precisely the scatter-add the dense assembler performs with `+=`.
pub(crate) struct SparseModalOperators {
    /// PEC-reduced curl-curl stiffness `K_int` (ε-independent for μ_r = 1).
    pub k: SparseColMat<usize, f64>,
    /// PEC-reduced ε-weighted mass `M_ε,int`.
    pub m_eps: SparseColMat<usize, f64>,
    /// PEC-reduced unweighted (uniform ε ≡ 1) mass `M₁,int`.
    pub m1: SparseColMat<usize, f64>,
    /// Interior DOF count (`dim`), the order of every returned matrix.
    pub dim: usize,
}

/// Build the interior-DOF renumbering: for each global DOF, `Some(interior_idx)`
/// if it survives the PEC restriction, else `None`. Also returns `dim`.
fn interior_renumber(interior_mask: &[bool]) -> (Vec<Option<usize>>, usize) {
    let mut map = Vec::with_capacity(interior_mask.len());
    let mut dim = 0usize;
    for &keep in interior_mask {
        if keep {
            map.push(Some(dim));
            dim += 1;
        } else {
            map.push(None);
        }
    }
    (map, dim)
}

/// Assemble the interior-restricted sparse `(K, M_ε, M₁)` for the **p=1**
/// Whitney/Nédélec modal pencil, directly from per-element 3×3 local blocks.
///
/// Equivalent (entry-for-entry) to
/// `apply_pec_2d(&assemble_2d_nedelec_with_epsilon(mesh, eps_r), …)` for `K`
/// and `M_ε`, and the same with uniform `ε ≡ 1` for `M₁` — but without the
/// dense intermediate. `interior_edge_mask` is aligned with [`TriMesh::edges`].
pub(crate) fn assemble_2d_nedelec_sparse_interior(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
) -> Result<SparseModalOperators, EigenError> {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let edges = mesh.edges();
    let n_edges = edges.len();
    assert_eq!(
        interior_edge_mask.len(),
        n_edges,
        "interior_edge_mask length must match edges count"
    );
    let tri_edges = mesh.tri_edges();
    let (renumber, dim) = interior_renumber(interior_edge_mask);

    // Reserve ~9 triplets per triangle for each matrix.
    let cap = 9 * mesh.n_tris();
    let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut m_eps_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut m1_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);

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
            let Some(ri) = renumber[gi as usize] else {
                continue;
            };
            for j in 0..3 {
                let (gj, sj) = row[j];
                let Some(rj) = renumber[gj as usize] else {
                    continue;
                };
                let s = (si as f64) * (sj as f64);
                // K (curl-curl) is ε-independent for μ_r = 1.
                k_trips.push(Triplet::new(ri, rj, s * k_local[i][j]));
                // ε weights the mass term ∫ ε N_i·N_j per element.
                m_eps_trips.push(Triplet::new(ri, rj, s * eps * m_local[i][j]));
                // M₁ is the uniform-ε ≡ 1 mass.
                m1_trips.push(Triplet::new(ri, rj, s * m_local[i][j]));
            }
        }
    }

    Ok(SparseModalOperators {
        k: triplets_to_sparse(dim, &k_trips)?,
        m_eps: triplets_to_sparse(dim, &m_eps_trips)?,
        m1: triplets_to_sparse(dim, &m1_trips)?,
        dim,
    })
}

/// Assemble the interior-restricted sparse `(K, M_ε, M₁)` for the **p=2**
/// Nédélec modal pencil, directly from per-element 8×8 local blocks.
///
/// Equivalent (entry-for-entry) to
/// `apply_pec_2d(&assemble_2d_nedelec2_with_epsilon(mesh, eps_r), …)` for `K`
/// and `M_ε` (and uniform `ε ≡ 1` for `M₁`), without the dense intermediate.
/// `interior_dof_mask` is aligned with the p=2 DOF numbering
/// ([`n_dof_2d_nedelec2`]); per-DOF orientation signs come from
/// [`tri_nedelec2_dofs`] / [`TRI_NEDELEC2_DOF_FLIPS`].
pub(crate) fn assemble_2d_nedelec2_sparse_interior(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
) -> Result<SparseModalOperators, EigenError> {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let n_edges = mesh.edges().len();
    let n_dof = 2 * n_edges + 2 * mesh.n_tris();
    assert_eq!(
        interior_dof_mask.len(),
        n_dof,
        "interior_dof_mask length ({}) must match p=2 DOF count ({})",
        interior_dof_mask.len(),
        n_dof
    );
    let tri_edges = mesh.tri_edges();
    let (renumber, dim) = interior_renumber(interior_dof_mask);

    let cap = 64 * mesh.n_tris();
    let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut m_eps_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut m1_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);

    for (tri_index, ((tri, row), &eps)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(eps_r.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        assert!(
            signed_area > 0.0,
            "rect_tri_mesh / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        for i in 0..8 {
            let (gi, si) = dofs[i];
            let Some(ri) = renumber[gi] else {
                continue;
            };
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let Some(rj) = renumber[gj] else {
                    continue;
                };
                let s = si * sj;
                k_trips.push(Triplet::new(ri, rj, s * k_local[i][j]));
                m_eps_trips.push(Triplet::new(ri, rj, s * eps * m_local[i][j]));
                m1_trips.push(Triplet::new(ri, rj, s * m_local[i][j]));
            }
        }
    }

    Ok(SparseModalOperators {
        k: triplets_to_sparse(dim, &k_trips)?,
        m_eps: triplets_to_sparse(dim, &m_eps_trips)?,
        m1: triplets_to_sparse(dim, &m1_trips)?,
        dim,
    })
}

/// 2D radial coordinate-stretch (UPML) constitutive data at a Cartesian
/// point, the 2D reduction of [`crate::scattering::upml_matched_tensors`]
/// (Epic #303 PML-A, issue #331).
///
/// In the absorbing annulus `r_pml_inner ≤ r ≤ r_outer` the radial stretch
/// is
///
/// ```text
///   s(r) = 1 − j·σ₀·((r − r_pml_inner)/d)²,   d = r_outer − r_pml_inner
/// ```
///
/// (the same quadratic-σ profile family as the 3D matched UPML; the `exp(+jωt)`
/// convention puts the loss in the **−j** imaginary part). The in-plane stretch
/// tensor in the radial / transverse eigenbasis is `Λ = diag(1/s, s)` — radial
/// eigenvalue `1/s`, transverse eigenvalue `s` — exactly the in-plane block of
/// the 3D `Λ = s·I + (1/s − s)·r̂r̂ᵀ` (the out-of-plane / `ẑ` eigenvalue of that
/// 3D tensor is `s`). Rotated into Cartesian (x, y),
///
/// ```text
///   Λ_t = s·I₂ + (1/s − s)·r̂r̂ᵀ        (2×2, in-plane)
/// ```
///
/// # What this returns
///
/// `(lambda_t, curl_weight)` where
/// - `lambda_t` is the 2×2 in-plane Cartesian `Λ_t` used to **sandwich** the
///   transverse mass term (`ε = ε_r·Λ_t`), and
/// - `curl_weight = 1/s = (Λ⁻¹)_zz` is the **scalar** stiffness weight on the
///   out-of-plane curl `(∇_t × N)·ẑ`. This is the `zz` component of the 3D
///   `Λ⁻¹` (the curl-curl `ν`-weight) restricted to the transverse problem,
///   where every basis curl is `ẑ`-directed, so the 3D `c_iᵀ Λ⁻¹ c_j` collapses
///   to `(Λ⁻¹)_zz · c_i c_j`.
///
/// Inside `r ≤ r_pml_inner` (or for `σ₀ = 0`) `s = 1`, so `Λ_t = I₂` and
/// `curl_weight = 1`: the assembly reduces bit-for-bit to the real path
/// embedded in `c64` with zero imaginary part.
pub fn pml_stretch_tensor_2d(
    centroid: [f64; 2],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
) -> ([[c64; 2]; 2], c64) {
    let one = c64::new(1.0, 0.0);
    let r = (centroid[0] * centroid[0] + centroid[1] * centroid[1]).sqrt();
    let identity = [[one, c64::new(0.0, 0.0)], [c64::new(0.0, 0.0), one]];
    if sigma_0 == 0.0 || r <= r_pml_inner {
        return (identity, one);
    }
    let d = (r_outer - r_pml_inner).max(1e-30);
    let u = ((r - r_pml_inner) / d).clamp(0.0, 1.0);
    let sigma = sigma_0 * u * u;
    let s = c64::new(1.0, -sigma);
    let s_inv = one / s;
    // r̂ in Cartesian; r > r_pml_inner > 0 here so r ≠ 0.
    let rx = centroid[0] / r;
    let ry = centroid[1] / r;
    // Λ_t = s·I + (1/s − s)·r̂r̂ᵀ.
    let coeff = s_inv - s;
    let lambda_t = [
        [
            s + coeff * c64::new(rx * rx, 0.0),
            coeff * c64::new(rx * ry, 0.0),
        ],
        [
            coeff * c64::new(ry * rx, 0.0),
            s + coeff * c64::new(ry * ry, 0.0),
        ],
    ];
    // Curl (stiffness) weight = (Λ⁻¹)_zz = 1/s.
    (lambda_t, s_inv)
}

/// Interior-restricted **complex** sparse Nédélec operators for the 2D
/// UPML modal pencil (Epic #303 PML-A, issue #331). The `c64` analogue of
/// [`SparseModalOperators`].
///
/// With `sigma_0 = 0` (or no PML-tagged triangles) the entries equal the
/// real [`SparseModalOperators`] embedded in `c64` with zero imaginary part,
/// entry for entry — see [`assemble_2d_nedelec2_pml_sparse_interior`].
//
// Consumed by the PML-B complex eigensolve (#332) via
// `dielectric_raw_candidates_p2_pml`.
pub(crate) struct SparseModalOperatorsComplex {
    /// PEC-reduced complex curl-curl stiffness `K_int` (UPML `1/s`-weighted
    /// on PML triangles, real elsewhere).
    pub k: SparseColMat<usize, c64>,
    /// PEC-reduced complex `ε_r·Λ_t`-weighted mass `M_ε,int`.
    pub m_eps: SparseColMat<usize, c64>,
    /// PEC-reduced complex `Λ_t`-weighted (uniform `ε ≡ 1`) mass `M₁,int`.
    pub m1: SparseColMat<usize, c64>,
    /// Interior DOF count (`dim`), the order of every returned matrix.
    pub dim: usize,
}

/// Assemble the interior-restricted **complex** p=2 Nédélec UPML operators
/// `(K, M_ε, M₁)` directly from per-element 8×8 local blocks (Epic #303
/// PML-A, issue #331).
///
/// This is the UPML-weighted, `c64` counterpart of
/// [`assemble_2d_nedelec2_sparse_interior`]. The scatter structure — DOF
/// numbering, per-DOF orientation signs ([`tri_nedelec2_dofs`] /
/// [`TRI_NEDELEC2_DOF_FLIPS`]), interior restriction, ε-weights-M rule — is
/// **identical**; the only addition is that on PML-tagged triangles the
/// local 8×8 `K`/`M` blocks are built with the per-element constant stretch
/// tensor from [`pml_stretch_tensor_2d`] (evaluated at the triangle
/// centroid, exactly as the 3D [`crate::scattering::build_matched_upml_materials`]
/// does per tet):
///
/// - the curl-curl stiffness scalar curl product is weighted by
///   `curl_weight = 1/s = (Λ⁻¹)_zz`, and
/// - the mass integrand `N_iᵀ N_j` is sandwiched as `N_iᵀ (ε_r·Λ_t) N_j`.
///
/// Non-PML triangles use the identity tensor (`Λ_t = I`, `curl_weight = 1`),
/// so they reproduce the real assembly's numbers exactly.
///
/// # Arguments
///
/// - `mesh` / `eps_r` / `interior_dof_mask` — as in
///   [`assemble_2d_nedelec2_sparse_interior`].
/// - `region_tags` — per-triangle region tag (length `mesh.n_tris()`); only
///   triangles tagged [`REGION_PML`] carry the stretch.
/// - `r_pml_inner` / `r_outer` — the PML annulus radii (the stretch ramps
///   quadratically from `r_pml_inner` to `r_outer`).
/// - `sigma_0` — UPML strength. `sigma_0 = 0` makes the layer transparent and
///   the operators reduce bit-for-bit to the real path.
///
/// # σ₀ = 0 reduction (load-bearing)
///
/// With `sigma_0 = 0` **or** no `REGION_PML` triangles, every local tensor is
/// the identity, every entry is real, and the returned `K`/`M_ε`/`M₁` equal
/// [`assemble_2d_nedelec2_sparse_interior`]'s output embedded in `c64` with
/// zero imaginary part — entry for entry. This proves the complex path does
/// not corrupt the validated real assembly. Asserted in a unit test.
///
/// The operators are **complex-symmetric** (`K = Kᵀ`, `M = Mᵀ` as complex
/// matrices — the bilinear-form convention, **not** Hermitian), matching the
/// Mie complex pencil.
///
/// Returns a [`SparseModalOperatorsComplex`].
///
/// # Panics
///
/// Panics if `eps_r.len()` or `region_tags.len()` ≠ `mesh.n_tris()`, or if
/// `interior_dof_mask.len()` ≠ [`n_dof_2d_nedelec2`].
// Consumed by the PML-B complex eigensolve (#332) via
// `dielectric_raw_candidates_p2_pml`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble_2d_nedelec2_pml_sparse_interior(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
) -> Result<SparseModalOperatorsComplex, EigenError> {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal the triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let n_edges = mesh.edges().len();
    let n_dof = 2 * n_edges + 2 * mesh.n_tris();
    assert_eq!(
        interior_dof_mask.len(),
        n_dof,
        "interior_dof_mask length ({}) must match p=2 DOF count ({})",
        interior_dof_mask.len(),
        n_dof
    );
    let tri_edges = mesh.tri_edges();
    let (renumber, dim) = interior_renumber(interior_dof_mask);

    let cap = 64 * mesh.n_tris();
    let mut k_trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(cap);
    let mut m_eps_trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(cap);
    let mut m1_trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(cap);

    for (tri_index, ((tri, row), &eps)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(eps_r.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];

        // Per-element constant stretch tensor, evaluated at the centroid (the
        // 2D analogue of build_matched_upml_materials' per-tet evaluation).
        // Non-PML triangles get the identity → real numbers embedded in c64.
        let (lambda_t, curl_weight) = if region_tags[tri_index] == REGION_PML {
            let cx = (coords[0][0] + coords[1][0] + coords[2][0]) / 3.0;
            let cy = (coords[0][1] + coords[1][1] + coords[2][1]) / 3.0;
            pml_stretch_tensor_2d([cx, cy], r_pml_inner, r_outer, sigma_0)
        } else {
            let one = c64::new(1.0, 0.0);
            let zero = c64::new(0.0, 0.0);
            ([[one, zero], [zero, one]], one)
        };

        let (k_local, m_local, signed_area) =
            tri_nedelec2_local_upml(&coords, &lambda_t, curl_weight);
        assert!(
            signed_area > 0.0,
            "disk_tri_mesh_pml / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        let eps_c = c64::new(eps, 0.0);
        for i in 0..8 {
            let (gi, si) = dofs[i];
            let Some(ri) = renumber[gi] else {
                continue;
            };
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let Some(rj) = renumber[gj] else {
                    continue;
                };
                let s = c64::new(si * sj, 0.0);
                k_trips.push(Triplet::new(ri, rj, s * k_local[i][j]));
                m_eps_trips.push(Triplet::new(ri, rj, s * eps_c * m_local[i][j]));
                m1_trips.push(Triplet::new(ri, rj, s * m_local[i][j]));
            }
        }
    }

    Ok(SparseModalOperatorsComplex {
        k: triplets_to_sparse_c64(dim, &k_trips)?,
        m_eps: triplets_to_sparse_c64(dim, &m_eps_trips)?,
        m1: triplets_to_sparse_c64(dim, &m1_trips)?,
        dim,
    })
}

/// UPML-weighted complex p=2 local element kernel: the `c64`, tensor-weighted
/// analogue of [`tri_nedelec2_local`] (Epic #303 PML-A, issue #331).
///
/// Reuses the *exact* same affine geometry, hierarchical basis (`eval`), and
/// [`TRI_QUAD_DEG4`] quadrature as [`tri_nedelec2_local`], but
/// - the stiffness integrand is `curl_weight · (∇×N_i)(∇×N_j)` (the scalar
///   out-of-plane curl product, weighted by `(Λ⁻¹)_zz = 1/s`), and
/// - the mass integrand is `N_iᵀ Λ_t N_j` (the 2×2 in-plane stretch tensor
///   sandwiched between the vector basis values).
///
/// With `lambda_t = I₂` and `curl_weight = 1` this returns exactly the real
/// `tri_nedelec2_local` blocks promoted to `c64` (zero imaginary part): the
/// quadrature, weights, and basis evaluation are byte-identical, and the only
/// added arithmetic is multiplication by the literal `1.0 + 0j`.
fn tri_nedelec2_local_upml(
    coords: &[[f64; 2]; 3],
    lambda_t: &[[c64; 2]; 2],
    curl_weight: c64,
) -> ([[c64; 8]; 8], [[c64; 8]; 8], f64) {
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let abs_det = det.abs();
    let area_abs = 0.5 * abs_det;

    let g = [
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

    let cross = |u: [f64; 2], v: [f64; 2]| -> f64 { u[0] * v[1] - u[1] * v[0] };

    // Identical hierarchical basis evaluation to tri_nedelec2_local.
    let eval = |lam: [f64; 3]| -> ([[f64; 2]; 8], [f64; 8]) {
        let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
        let whitney = |a: usize, b: usize, la: f64, lb: f64| -> ([f64; 2], f64) {
            let val = [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]];
            let curl = 2.0 * cross(g[a], g[b]);
            (val, curl)
        };
        let (w0, cw0) = whitney(0, 1, l0, l1);
        let (w1, cw1) = whitney(0, 2, l0, l2);
        let (w2, cw2) = whitney(1, 2, l1, l2);
        let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
            [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
        };
        let q0 = qgrad(0, 1, l0, l1);
        let q1 = qgrad(0, 2, l0, l2);
        let q2 = qgrad(1, 2, l1, l2);
        let bubble = |w: [f64; 2], cw: f64, c: usize, lc: f64| -> ([f64; 2], f64) {
            let val = [lc * w[0], lc * w[1]];
            let curl = cross(g[c], w) + lc * cw;
            (val, curl)
        };
        let (i0, ci0) = bubble(w0, cw0, 2, l2);
        let (i1, ci1) = bubble(w2, cw2, 0, l0);
        let vals = [w0, q0, w1, q1, w2, q2, i0, i1];
        let curls = [cw0, 0.0, cw1, 0.0, cw2, 0.0, ci0, ci1];
        (vals, curls)
    };

    // Detect the identity tensor (no PML, or σ₀ = 0). In that case run the
    // **exact** real arithmetic of tri_nedelec2_local and promote to c64 with
    // a single trailing `1 + 0j` multiply, so the non-PML path is bit-for-bit
    // equal to the validated real assembly.
    let one = c64::new(1.0, 0.0);
    let zero = c64::new(0.0, 0.0);
    let is_identity = curl_weight == one
        && lambda_t[0][0] == one
        && lambda_t[1][1] == one
        && lambda_t[0][1] == zero
        && lambda_t[1][0] == zero;

    let mut k_local = [[zero; 8]; 8];
    let mut m_local = [[zero; 8]; 8];

    if is_identity {
        // Bit-for-bit mirror of tri_nedelec2_local's accumulation.
        let mut k_real = [[0.0_f64; 8]; 8];
        let mut m_real = [[0.0_f64; 8]; 8];
        for row in TRI_QUAD_DEG4.iter() {
            let lam = [row[0], row[1], row[2]];
            let w = row[3] * area_abs;
            let (vals, curls) = eval(lam);
            for i in 0..8 {
                for j in 0..8 {
                    k_real[i][j] += w * curls[i] * curls[j];
                    m_real[i][j] += w * (vals[i][0] * vals[j][0] + vals[i][1] * vals[j][1]);
                }
            }
        }
        for i in 0..8 {
            for j in 0..8 {
                k_local[i][j] = curl_weight * c64::new(k_real[i][j], 0.0);
                m_local[i][j] = c64::new(m_real[i][j], 0.0);
            }
        }
        return (k_local, m_local, area);
    }

    // PML path: accumulate the per-Cartesian-component mass and the scalar
    // curl product in f64, then apply the constant per-element tensor weight
    // (the 2D analogue of the 3D `sandwich` against the summed constant
    // curls/grads). With M^{ab}_ij = ∫ N_i,a N_j,b, the Λ_t-sandwiched mass is
    //   M_ij = Σ_{a,b} Λ_t[a][b] · M^{ab}_ij,
    // and the stiffness is `(Λ⁻¹)_zz · ∫ (∇×N_i)(∇×N_j)`.
    let mut k_real = [[0.0_f64; 8]; 8];
    let mut m_xx = [[0.0_f64; 8]; 8];
    let mut m_xy = [[0.0_f64; 8]; 8];
    let mut m_yx = [[0.0_f64; 8]; 8];
    let mut m_yy = [[0.0_f64; 8]; 8];
    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area_abs;
        let (vals, curls) = eval(lam);
        for i in 0..8 {
            for j in 0..8 {
                k_real[i][j] += w * curls[i] * curls[j];
                m_xx[i][j] += w * vals[i][0] * vals[j][0];
                m_xy[i][j] += w * vals[i][0] * vals[j][1];
                m_yx[i][j] += w * vals[i][1] * vals[j][0];
                m_yy[i][j] += w * vals[i][1] * vals[j][1];
            }
        }
    }
    for i in 0..8 {
        for j in 0..8 {
            k_local[i][j] = curl_weight * c64::new(k_real[i][j], 0.0);
            m_local[i][j] = lambda_t[0][0] * c64::new(m_xx[i][j], 0.0)
                + lambda_t[0][1] * c64::new(m_xy[i][j], 0.0)
                + lambda_t[1][0] * c64::new(m_yx[i][j], 0.0)
                + lambda_t[1][1] * c64::new(m_yy[i][j], 0.0);
        }
    }

    (k_local, m_local, area)
}

/// Build a square `dim × dim` complex `SparseColMat` from `c64` COO triplets,
/// summing duplicate `(row, col)` entries (the `c64` analogue of
/// [`triplets_to_sparse`]).
fn triplets_to_sparse_c64(
    dim: usize,
    trips: &[Triplet<usize, usize, c64>],
) -> Result<SparseColMat<usize, c64>, EigenError> {
    SparseColMat::<usize, c64>::try_new_from_triplets(dim, dim, trips).map_err(|e| {
        EigenError::FaerGevd(format!("waveguide_modes complex sparse assembly: {e:?}"))
    })
}

/// Build a square `dim × dim` `SparseColMat` from COO triplets, summing
/// duplicate `(row, col)` entries (the scatter-add the dense assembler does).
fn triplets_to_sparse(
    dim: usize,
    trips: &[Triplet<usize, usize, f64>],
) -> Result<SparseColMat<usize, f64>, EigenError> {
    SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, trips)
        .map_err(|e| EigenError::FaerGevd(format!("waveguide_modes sparse assembly: {e:?}")))
}

/// The standard-form pencil operator `A = k₀² M_ε − K`, assembled as a fresh
/// sparse matrix from two interior-restricted sparse operators sharing the
/// same sparsity pattern. Mirrors the dense `a_global = k0²·M_ε − K` step,
/// then `try_new_from_triplets` sums the (identically-located) contributions.
fn sparse_pencil_a(
    k: SparseColMatRef<'_, usize, f64>,
    m_eps: SparseColMatRef<'_, usize, f64>,
    k0_sq: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = k.nrows();
    let nnz = k.col_ptr()[n] + m_eps.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(nnz);
    let push = |trips: &mut Vec<Triplet<usize, usize, f64>>,
                a: SparseColMatRef<'_, usize, f64>,
                scale: f64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for kk in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[kk], j, scale * v[kk]));
            }
        }
    };
    push(&mut trips, m_eps, k0_sq);
    push(&mut trips, k, -1.0);
    triplets_to_sparse(n, &trips)
}

/// Compute the quadratic form `xᵀ A x` for a sparse `A` and dense vector `x`.
fn sparse_quadratic_form(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> f64 {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut acc = 0.0_f64;
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            acc += x[ri[k]] * v[k] * xj;
        }
    }
    acc
}

/// Build the p=2 PEC/Dirichlet interior-DOF mask for a rectangle
/// `[0,W] × [0,H]`, extending [`rect_pec_interior_edges`] to the p=2 DOF
/// layout of [`assemble_2d_nedelec2_with_epsilon`].
///
/// An entry is `true` if its global DOF is **interior** (free) and `false`
/// if it lies on the PEC boundary (Dirichlet-constrained to zero):
///
/// - Both edge DOFs of a global edge (the Whitney `W` and the gradient
///   `Q`) follow that edge's first-order interior status: a wall-aligned
///   edge contributes **both** of its DOFs to the boundary set; an interior
///   edge keeps both as interior. (The tangential trace `n × E = 0` on a
///   wall-aligned edge kills the entire edge-tangential field there, not
///   just its Whitney component.)
/// - Both interior (face) bubble DOFs of every triangle are always
///   interior — face bubbles vanish on element boundaries by construction.
///
/// Returns a mask aligned with the p=2 global DOF numbering: edge DOFs
/// first (`2·n_edges` of them, `2e`/`2e+1` per global edge), then interior
/// DOFs (`2·n_tris` of them, `2t`/`2t+1` per triangle). Length is
/// [`n_dof_2d_nedelec2`].
pub fn rect_pec_interior_dofs2(mesh: &TriMesh, width: f64, height: f64) -> Vec<bool> {
    let (_edges, edge_interior) = rect_pec_interior_edges(mesh, width, height);
    interior_dofs2_from_edge_mask(mesh, &edge_interior)
}

/// Build the p=2 PEC/Dirichlet interior-DOF mask for a [`disk_tri_mesh`] of
/// outer radius `outer_radius`, extending [`disk_pec_interior_edges`] to the
/// p=2 DOF layout. See [`rect_pec_interior_dofs2`] for the layout and rule.
pub fn disk_pec_interior_dofs2(mesh: &TriMesh, outer_radius: f64) -> Vec<bool> {
    let (_edges, edge_interior) = disk_pec_interior_edges(mesh, outer_radius);
    interior_dofs2_from_edge_mask(mesh, &edge_interior)
}

/// Expand a per-global-edge interior mask (aligned with [`TriMesh::edges`])
/// into the full p=2 interior-DOF mask: each edge contributes both of its
/// DOFs with the edge's interior status, and all `2·n_tris` interior bubble
/// DOFs are interior.
fn interior_dofs2_from_edge_mask(mesh: &TriMesh, edge_interior: &[bool]) -> Vec<bool> {
    let n_edges = edge_interior.len();
    debug_assert_eq!(n_edges, mesh.edges().len());
    let mut mask = Vec::with_capacity(2 * n_edges + 2 * mesh.n_tris());
    for &interior in edge_interior {
        mask.push(interior); // Whitney W DOF
        mask.push(interior); // gradient Q DOF
    }
    for _ in 0..mesh.n_tris() {
        mask.push(true); // interior bubble I₀
        mask.push(true); // interior bubble I₁
    }
    mask
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

// ===========================================================================
// Phase-2.5C (Epic #318): order-2 (p=2) de-Rham gradient nullspace
// ===========================================================================

/// Local p=2 scalar (H¹ Lagrange, order 2) basis gradients on a triangle,
/// evaluated at a barycentric point. Returns the 6 gradients in the order
/// `[φ₀, φ₁, φ₂, φ_{01}, φ_{02}, φ_{12}]` (3 vertex functions, then 3 edge
/// functions in [`TRI_LOCAL_EDGES`] order).
///
/// The quadratic Lagrange basis is, in barycentrics,
/// `φ_a = λ_a (2λ_a − 1)` (vertices) and `φ_{ab} = 4 λ_a λ_b` (edges), so
/// `∇φ_a = (4λ_a − 1) g_a` and `∇φ_{ab} = 4 (λ_a g_b + λ_b g_a)`.
///
/// Note `∇φ_{ab} = 4 Q_{ab}` is exactly four times the p=2 *gradient* edge
/// function `Q = ∇(λ_a λ_b)` of [`tri_nedelec2_local`] — the algebraic
/// statement that the d⁰ image of an interior scalar edge DOF is the
/// corresponding `Q` edge DOF, which is why the p=2 gradient nullspace
/// gains one dimension per interior edge on top of the p=1 per-node count.
fn tri_scalar2_grads(g: &[[f64; 2]; 3], lam: [f64; 3]) -> [[f64; 2]; 6] {
    let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
    let l = [l0, l1, l2];
    let mut out = [[0.0_f64; 2]; 6];
    // Vertex functions: ∇φ_a = (4λ_a − 1) g_a.
    for a in 0..3 {
        let s = 4.0 * l[a] - 1.0;
        out[a] = [s * g[a][0], s * g[a][1]];
    }
    // Edge functions: ∇φ_{ab} = 4 (λ_a g_b + λ_b g_a).
    for (k, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
        out[3 + k] = [
            4.0 * (l[a] * g[b][0] + l[b] * g[a][0]),
            4.0 * (l[a] * g[b][1] + l[b] * g[a][1]),
        ];
    }
    out
}

/// Build the **algebraic** discrete-gradient `d⁰` mapping the order-2
/// scalar (H¹ Lagrange) space into the p=2 Nédélec edge space, restricted
/// to interior DOFs, for the generalized de-Rham nullspace dimension at
/// p=2.
///
/// At first order `restrict_gradient_dense_2d` builds `d⁰` combinatorially
/// (`±1` at edge endpoints) because the Whitney edge DOF functional is the
/// tangential edge integral `∫ ∇φ·t = φ(b) − φ(a)`. At p=2 there is no such
/// closed combinatorial form for the second edge DOF (`Q`) and the two
/// interior bubbles, so we build `d⁰` **algebraically** by expressing the
/// gradient of each scalar-p2 basis function in the local p=2 edge basis
/// via an L²(element) projection: solve `M_loc c = b`, with
/// `b_i = ∫ N_i · ∇φ_j` and `M_loc` the local Nédélec mass. Because the
/// first-kind order-2 de-Rham sequence is **exact**, `∇φ_j` lies in the
/// edge space and this projection is exact (the residual
/// `‖∇φ_j − Σ_i c_i N_i‖` is ~machine zero — asserted in the unit test).
///
/// The scalar space is numbered `[nodes…, edge-midpoints…]`:
/// vertex DOF `a` is node `a`; edge-midpoint DOF for global edge `e` is
/// `n_nodes + e`. The columns are filtered to interior scalar DOFs via the
/// node mask (vertices) and edge mask (edge midpoints); the rows are the
/// interior p=2 edge DOFs (same layout as `interior_dofs2_*`). The returned
/// matrix's rank is the p=2 gradient-nullspace dimension.
fn restrict_gradient_dense_2d_p2(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    interior_node_mask: &[bool],
    interior_edge_mask: &[bool],
) -> Mat<f64> {
    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();
    let n_dof = 2 * n_edges + 2 * mesh.n_tris();
    assert_eq!(interior_dof_mask.len(), n_dof);
    assert_eq!(interior_node_mask.len(), mesh.n_nodes());
    assert_eq!(interior_edge_mask.len(), n_edges);

    // Scalar-p2 global numbering: vertices [0..n_nodes), then edge
    // midpoints [n_nodes .. n_nodes + n_edges). Interior columns keep
    // interior nodes and interior edges.
    let n_nodes = mesh.n_nodes();
    let mut scalar_to_interior: Vec<Option<usize>> = vec![None; n_nodes + n_edges];
    let mut n_scalar_interior = 0usize;
    for (node, &keep) in interior_node_mask.iter().enumerate() {
        if keep {
            scalar_to_interior[node] = Some(n_scalar_interior);
            n_scalar_interior += 1;
        }
    }
    for (edge, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            scalar_to_interior[n_nodes + edge] = Some(n_scalar_interior);
            n_scalar_interior += 1;
        }
    }

    // Edge-DOF (row) interior renumbering.
    let mut dof_to_interior: Vec<Option<usize>> = vec![None; n_dof];
    let mut n_edge_interior = 0usize;
    for (dof, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            dof_to_interior[dof] = Some(n_edge_interior);
            n_edge_interior += 1;
        }
    }

    let mut d0 = Mat::<f64>::zeros(n_edge_interior, n_scalar_interior);

    for (tri_index, (tri, row)) in mesh.tris.iter().zip(tri_edges.iter()).enumerate() {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        // Local p=2 mass and the affine gradient gram, plus the local
        // basis evaluator (re-derived here to keep the kernel private).
        let (_k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        debug_assert!(signed_area > 0.0);

        // Affine barycentric gradients (same as tri_nedelec2_local).
        let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
            - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
        let area_abs = 0.5 * det.abs();
        let g = [
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

        // Local edge basis evaluator (mirrors tri_nedelec2_local::eval but
        // returns only the 8 vector values).
        let eval_vecs = |lam: [f64; 3]| -> [[f64; 2]; 8] {
            let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
            let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
            };
            let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
            };
            let w0 = whitney(0, 1, l0, l1);
            let w1 = whitney(0, 2, l0, l2);
            let w2 = whitney(1, 2, l1, l2);
            let q0 = qgrad(0, 1, l0, l1);
            let q1 = qgrad(0, 2, l0, l2);
            let q2 = qgrad(1, 2, l1, l2);
            let i0 = [l2 * w0[0], l2 * w0[1]];
            let i1 = [l0 * w2[0], l0 * w2[1]];
            [w0, q0, w1, q1, w2, q2, i0, i1]
        };

        // Build the RHS b_{i,s} = ∫ N_i · ∇φ_s over the element, for each
        // of the 6 scalar basis functions s. ∇φ_s is affine in λ; quadrature
        // is exact at degree 4.
        let mut rhs = [[0.0_f64; 6]; 8];
        for qrow in TRI_QUAD_DEG4.iter() {
            let lam = [qrow[0], qrow[1], qrow[2]];
            let w = qrow[3] * area_abs;
            let vecs = eval_vecs(lam);
            let sgrads = tri_scalar2_grads(&g, lam);
            for i in 0..8 {
                for s in 0..6 {
                    rhs[i][s] += w * (vecs[i][0] * sgrads[s][0] + vecs[i][1] * sgrads[s][1]);
                }
            }
        }

        // Solve M_loc c_s = b_s for the edge-basis coefficients of ∇φ_s.
        let coeffs = solve_8x6(&m_local, &rhs);

        // Map local scalar DOF s → global scalar index, local edge DOF i →
        // global edge DOF, apply orientation signs, and scatter into d⁰.
        let scalar_global = [
            tri[0] as usize,
            tri[1] as usize,
            tri[2] as usize,
            n_nodes + row[0].0 as usize,
            n_nodes + row[1].0 as usize,
            n_nodes + row[2].0 as usize,
        ];
        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);

        for s in 0..6 {
            let Some(col) = scalar_to_interior[scalar_global[s]] else {
                continue;
            };
            for i in 0..8 {
                let (gdof, sign) = dofs[i];
                let Some(rowi) = dof_to_interior[gdof] else {
                    continue;
                };
                // The global coefficient picks up the edge-DOF orientation
                // sign so that ∇φ_s expressed in the *global* basis is
                // assembled consistently (the local coefficient multiplies
                // the local basis function; the global DOF = sign · local).
                d0[(rowi, col)] += sign * coeffs[i][s];
            }
        }
    }
    d0
}

/// Solve `A c = b` for an 8×8 SPD `A` and 6 right-hand sides (the local
/// p=2 mass is symmetric positive-definite), returning the 8×6 coefficient
/// block. Plain Cholesky — the system is tiny and fixed-size.
// The Cholesky / triangular-solve index `k` legitimately indexes two
// different rows (`l[i][k]` and `l[j][k]`), so the range loop is clearer
// than an iterator rewrite.
#[allow(clippy::needless_range_loop)]
fn solve_8x6(a: &[[f64; 8]; 8], b: &[[f64; 6]; 8]) -> [[f64; 6]; 8] {
    // Cholesky A = L Lᵀ.
    let mut l = [[0.0_f64; 8]; 8];
    for i in 0..8 {
        for j in 0..=i {
            let mut sum = a[i][j];
            for k in 0..j {
                sum -= l[i][k] * l[j][k];
            }
            if i == j {
                l[i][j] = sum.max(0.0).sqrt();
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    // Solve for each RHS column: L y = b, Lᵀ c = y.
    let mut c = [[0.0_f64; 6]; 8];
    for s in 0..6 {
        let mut y = [0.0_f64; 8];
        for i in 0..8 {
            let mut sum = b[i][s];
            for k in 0..i {
                sum -= l[i][k] * y[k];
            }
            y[i] = sum / l[i][i];
        }
        for i in (0..8).rev() {
            let mut sum = y[i];
            for k in (i + 1)..8 {
                sum -= l[k][i] * c[k][s];
            }
            c[i][s] = sum / l[i][i];
        }
    }
    c
}

/// Order-aware (p=2) generalization of [`spurious_dim_2d`]: the
/// gradient-nullspace dimension of the p=2 Nédélec curl-curl pencil after
/// PEC reduction, equal to `rank(d⁰_interior)` of the **order-2 scalar →
/// p2 edge** discrete gradient.
///
/// By the exact first-kind order-2 de-Rham sequence the gradient image is
/// injective on the interior scalar DOFs, so the rank equals the number of
/// interior scalar-p2 DOFs:
///
/// ```text
///   spurious_dim_2d_p2 = (interior nodes) + (interior edges).
/// ```
///
/// (Compare p=1, where it is just the interior-node count.) The unit test
/// `p2_spurious_dim_counts_interior_scalar_dofs` pins this against the mesh
/// counts on a known small mesh.
pub fn spurious_dim_2d_p2(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    interior_node_mask: &[bool],
    interior_edge_mask: &[bool],
) -> usize {
    let d0 = restrict_gradient_dense_2d_p2(
        mesh,
        interior_dof_mask,
        interior_node_mask,
        interior_edge_mask,
    );
    rank_via_svd_2d(&d0, 1e-12)
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

/// Mesh-independent **reference-integral gauge** for a transverse modal
/// eigenvector (issue #300, the downstream half of friction artifact
/// #18). Replaces the [`pin_eigenvector_sign`] argmax convention at the
/// modal-wrapper layer.
///
/// # Why the argmax pin is not enough across meshes
///
/// [`pin_eigenvector_sign`] (issue #262) is deterministic *per call*,
/// but its pivot — the single largest-magnitude edge DOF — is a property
/// of the *discretization*, not the *continuous mode*. When the mesh is
/// refined, the argmax DOF can jump to a different edge (e.g. for higher
/// modes whose dominant edge sits near a face the two meshes resolve
/// differently), flipping the pinned sign and therefore the *complex*
/// S-matrix entries downstream — even though both runs are individually
/// deterministic and the gauge-invariant magnitudes are stable. PR #261
/// surfaced exactly this (`nx = 10 → nx = 16` flipped raw S entries),
/// forcing the C2 mode-matching test to compare magnitudes only.
///
/// # The reference-integral convention
///
/// Instead of pivoting on a single DOF, fix the gauge by **integrating
/// the eigenvector against a fixed continuous reference profile** and
/// rotating it so that projection is real-positive:
///
/// ```text
/// p = ⟨e, r⟩ = Σ_i e_i · r_i,    where  r_i ≈ ∫_{edge i} F · t̂ dl
/// ```
///
/// `F(x, y)` is a smooth reference vector field evaluated at each edge
/// midpoint and dotted with the (global-oriented) edge tangent, so `r_i`
/// is the Whitney/Nédélec DOF the analytic field `F` would produce on
/// edge `i`. The projection `p` is a **continuous functional of the
/// mode** (a quadrature of `∫ e · F dS`), so it converges as the mesh is
/// refined and does *not* hinge on which discrete DOF happens to be
/// largest. We flip `e → −e` iff `p < 0`, pinning the sign consistently
/// regardless of mesh.
///
/// For a **complex** eigenvector (the dielectric / complex-pencil paths)
/// the same construction pins the full phase: rotate by the unit-modulus
/// scalar `e^{−i·arg(p)}` so `p` becomes real-positive. The real
/// transverse pencil here only needs the `±1` specialization, but the
/// convention is stated complex so the two paths share one contract.
///
/// # Robustness: a small reference basis
///
/// A single fixed `F` can be (near-)orthogonal to some modes — e.g. a
/// uniform x-directed field has zero net projection onto a mode that is
/// x-antisymmetric (TE₂₀-like). To stay well-defined for every mode we
/// try an ordered list of reference fields and use the **first** whose
/// projection magnitude clears a relative floor. The list is fixed and
/// mesh-independent, so two meshes resolving the same physical mode
/// select the same reference and therefore the same sign. If *every*
/// reference projection is negligible (degenerate / pathological case)
/// we fall back to [`pin_eigenvector_sign`] so the gauge is always
/// defined.
///
/// # Invariants
///
/// The rotation is a unit-modulus scalar (here `±1`), so it is
/// **norm-preserving**: `eᵀ M e` and the set-wise `e_iᵀ M e_j` Gram
/// entries are unchanged. M-orthonormality of the returned set is
/// therefore preserved exactly (see the unit test
/// `reference_gauge_preserves_orthonormality`).
fn gauge_fix_eigenvector(mesh: &TriMesh, edges: &[[u32; 2]], e_edges: &mut [f64]) {
    debug_assert_eq!(
        edges.len(),
        e_edges.len(),
        "edge count must match eigenvector length"
    );

    // Smooth reference vector fields F(x, y), tried in order. Each entry
    // maps an edge midpoint (x, y) to a 2-D field vector. The list is
    // deliberately simple, fixed, and ordered low-frequency-first so the
    // dominant (fundamental) mode locks onto the first field and higher
    // modes — orthogonal to it — fall through to a field they overlap.
    //
    // The mesh extent only sets a length scale for the trig references;
    // it does not affect the *sign* of the projection (a positive
    // overall rescaling of F leaves sign(p) intact), so cross-mesh
    // sign consistency is preserved even if the bounding box is read off
    // a slightly different node set.
    let (mut xmin, mut ymin) = (f64::INFINITY, f64::INFINITY);
    let (mut xmax, mut ymax) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &mesh.nodes {
        xmin = xmin.min(p[0]);
        ymin = ymin.min(p[1]);
        xmax = xmax.max(p[0]);
        ymax = ymax.max(p[1]);
    }
    let lx = (xmax - xmin).max(f64::EPSILON);
    let ly = (ymax - ymin).max(f64::EPSILON);
    // sx, sy ∈ [0, 1] are the normalized in-box coordinates of a point.
    //
    // The reference fields are chosen to mirror the transverse-E shapes
    // of the lowest rectangular-guide modes so that each mode locks onto
    // a *physically matched* reference (a strong, mesh-stable overlap)
    // rather than a near-zero accidental one. For a `[0, a] × [0, b]`
    // metallic guide the lowest TE/TM modes have, up to normalization:
    //
    //   TE_{m,0}:  E_y ∝ sin(m π x / a),     E_x = 0
    //   TE_{0,n}:  E_x ∝ sin(n π y / b),     E_y = 0
    //
    // A y-directed field `∝ sin(m π sx)` therefore overlaps TE_{m,0}
    // strongly while integrating to ≈ 0 against every other listed mode
    // (`sin` orthogonality in x), and symmetrically for the x-directed
    // `∝ sin(n π sy)` references and TE_{0,n}. Crucially a *uniform*
    // field has zero net overlap with TE_{m,0} for any m (the sin
    // integrates to 0 over the full span), which is why the earlier
    // uniform/`cos` references left TE₂₀ ungauged and falling through to
    // the mesh-unstable argmax fallback — the bug this set fixes.
    type RefField = fn(f64, f64) -> [f64; 2];
    let refs: [RefField; 6] = [
        // 1. y-field × sin(π sx): matches TE₁₀ (fundamental).
        |sx, _sy| [0.0, (std::f64::consts::PI * sx).sin()],
        // 2. y-field × sin(2π sx): matches TE₂₀ (x-antisymmetric;
        //    orthogonal to ref 1).
        |sx, _sy| [0.0, (2.0 * std::f64::consts::PI * sx).sin()],
        // 3. x-field × sin(π sy): matches TE₀₁.
        |_sx, sy| [(std::f64::consts::PI * sy).sin(), 0.0],
        // 4. x-field × sin(2π sy): matches TE₀₂.
        |_sx, sy| [(2.0 * std::f64::consts::PI * sy).sin(), 0.0],
        // 5/6. Uniform x and y catch-alls (for any residual mode with a
        //    net directed component, e.g. mixed TM profiles).
        |_sx, _sy| [1.0, 0.0],
        |_sx, _sy| [0.0, 1.0],
    ];

    // Energy scale of the eigenvector (so the projection floor is
    // relative to the vector, not an absolute magnitude that depends on
    // the M-normalization length scale).
    let e_scale = e_edges
        .iter()
        .fold(0.0_f64, |acc, &x| acc + x * x)
        .sqrt()
        .max(f64::EPSILON);

    for f in &refs {
        let mut proj = 0.0_f64;
        let mut ref_scale = 0.0_f64;
        for (i, &ei) in e_edges.iter().enumerate() {
            if ei == 0.0 {
                continue; // PEC-eliminated edge carries an exact zero.
            }
            let [a, b] = edges[i];
            let pa = mesh.nodes[a as usize];
            let pb = mesh.nodes[b as usize];
            // Global-oriented tangent (a → b; a < b by edge convention).
            let t = [pb[0] - pa[0], pb[1] - pa[1]];
            let mx = 0.5 * (pa[0] + pb[0]);
            let my = 0.5 * (pa[1] + pb[1]);
            let sx = (mx - xmin) / lx;
            let sy = (my - ymin) / ly;
            let fv = f(sx, sy);
            // r_i ≈ ∫_edge F · t̂ dl  (midpoint rule).
            let ri = fv[0] * t[0] + fv[1] * t[1];
            proj += ei * ri;
            ref_scale += ri * ri;
        }
        let ref_scale = ref_scale.sqrt();
        // Relative floor: require the projection to be a non-trivial
        // fraction of the Cauchy–Schwarz ceiling |e|·|r|. A field that is
        // orthogonal to this mode produces proj ≈ 0 and is skipped.
        let floor = 1e-6 * e_scale * ref_scale;
        if proj.abs() > floor {
            if proj < 0.0 {
                for x in e_edges.iter_mut() {
                    *x = -*x;
                }
            }
            return;
        }
    }

    // Degenerate fallback: no reference overlapped. Use the argmax pin so
    // the gauge is always defined (still deterministic per call).
    pin_eigenvector_sign(e_edges);
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
/// # Sign / gauge convention (issue #300, superseding the #262 pin)
///
/// Each returned eigenvector's sign is pinned by a **reference-integral
/// gauge** ([`gauge_fix_eigenvector`]): the eigenvector is rotated so its
/// projection onto a fixed continuous reference profile is real-positive.
/// Concretely, a smooth reference vector field `F(x, y)` is sampled at
/// each edge midpoint to form the Whitney DOF it would produce, and the
/// eigenvector is negated iff its inner product with that reference is
/// negative.
///
/// This gives a gauge that is reproducible **across mesh refinements at
/// the level of the raw complex S-matrix entries**, not merely
/// deterministic per call. The earlier convention (issue #262,
/// [`pin_eigenvector_sign`]) pinned on the single largest-magnitude DOF;
/// because that pivot DOF is a property of the discretization, it could
/// jump to a different edge between meshes and flip the sign — so the C2
/// mode-matching test had to compare gauge-invariant *magnitudes* (PR
/// #261 documented the `nx = 10 → nx = 16` flip). The reference-integral
/// projection is a quadrature of the *continuous* functional `∫ e · F dS`,
/// so it converges with the mesh instead of jumping, pinning both sign
/// and (in the complex generalization) phase consistently regardless of
/// mesh. See [`gauge_fix_eigenvector`] for the convention's rationale and
/// the robustness handling for modes orthogonal to a given reference.
///
/// All gauge-invariant observables — eigenvalues `λ = k_c²`, modal
/// energies `‖e‖²_M = 1`, set-wise M-orthonormality `e_iᵀ M e_j`,
/// reciprocity, power-conservation column sums of the rank-N S-matrix
/// — are unaffected: the rotation is a unit-modulus (here `±1`) scalar
/// and therefore norm-preserving.
///
/// **Note**: this metallic path is real-valued, so the gauge reduces to
/// a sign. The complex eigenvector paths (`complex_eigen.rs` /
/// `complex_lanczos.rs`, and the [`DielectricMode`] solver) still use the
/// [`pin_eigenvector_sign`] argmax pin; extending the reference-integral
/// phase gauge to them is the documented complex generalization in
/// [`gauge_fix_eigenvector`] but is out of scope for issue #300.
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
/// added the deterministic largest-magnitude sign pin; issue #300
/// replaced it (at this wrapper layer) with the reference-integral gauge
/// documented above so the raw complex S-matrix is cross-mesh
/// reproducible, not just gauge-invariant in magnitude.
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
    let n_edges = edges.len();
    assert_eq!(
        interior_edge_mask.len(),
        n_edges,
        "interior_edge_mask length must match edges count"
    );

    // Interior-restricted sparse K and M (uniform ε ≡ 1, which equals the
    // ε-free `assemble_2d_nedelec` bit-for-bit), assembled directly.
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let ops = assemble_2d_nedelec_sparse_interior(mesh, &eps_ones, interior_edge_mask)?;
    let dim = ops.dim;
    let k_sparse = ops.k;
    let m_sparse = ops.m1;

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
                // Reference-integral gauge (issue #300, replacing the
                // issue-#262 argmax pin at the wrapper layer): rotate the
                // eigenvector so its projection onto a fixed continuous
                // reference profile is real-positive. Unlike the argmax
                // pin (whose pivot DOF can jump between meshes and flip
                // the sign), this projects onto a mesh-independent
                // functional of the *continuous* mode, so the pinned sign
                // — and the downstream complex S-matrix entries — are
                // reproducible across mesh refinements. The flip is a
                // unit-modulus (±1) rotation, so it preserves eᵀ M e and
                // the set-wise e_iᵀ M e_j Gram (M-orthonormality intact).
                gauge_fix_eigenvector(mesh, edges, &mut e_edges);
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
    let curl_floor = physical_curl_floor();

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

    // Assemble the interior-restricted sparse operators directly (no dense
    // N×N round-trip): K (curl-curl), M_ε (ε-weighted mass) and M₁ (uniform
    // ε ≡ 1 mass), each already PEC-reduced to the interior edge DOFs. The
    // sparse nonzeros equal the previous dense
    // `apply_pec_2d(&assemble_2d_nedelec_with_epsilon …)` output entry for
    // entry.
    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec_sparse_interior(mesh, eps_r, interior_edge_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_int = ops.k;
    let m_eps_int = ops.m_eps;

    // Standard-form pencil operator A = k₀² M_ε − K, assembled sparsely.
    let a_sparse = sparse_pencil_a(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;
    let m1_sparse = ops.m1;

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

    // r = (xᵀ K x) / (k₀² xᵀ M_ε x), computed via sparse quadratic forms.
    let curl_ratio = |x_interior: &[f64]| -> f64 {
        let xkx = sparse_quadratic_form(k_int.as_ref(), x_interior);
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
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
/// # Why a fixed floor (no adaptive widening)
///
/// A data-driven floor that widens into the largest multiplicative gap in
/// the *full* candidate spectrum is fragile: an **out-of-window** high-curl
/// eigenpair *above* the `n_core` ceiling (e.g. a radiation/continuum spike
/// at `n_eff ≈ 3.52`, `r ≈ 35`, far above the genuine band ceiling
/// `r ≈ 0.19`) creates an enormous (`~180×`) gap, and a widening rule raises
/// the floor into it — rejecting *every* genuine in-window mode and
/// returning zero bound modes. That regression was observed at
/// `ny=100/nx=5`. Because the calibrated gap `(1.7×10⁻², 8.5×10⁻²)` holds at
/// every resolution swept (40/50/60/70/80/90/100/120), the fixed `3×10⁻²`
/// floor alone is the robust choice: it rejects the pure gradient nullspace
/// (`r ≈ 10⁻¹⁶`) and the weakly-resolved spurious band (`r ≤ 1.7×10⁻²`)
/// while keeping the genuine guided band (`r ≥ 8.5×10⁻²`), and can never be
/// pushed above the genuine band by an out-of-window spike. A pure gradient
/// mode at `r ≈ 0` is therefore always rejected.
fn physical_curl_floor() -> f64 {
    // Calibrated base floor: centred in the measured gap between the
    // weakly-resolved spurious band (≤ ~1.7e-2) and the genuine guided
    // band (≥ ~8.5e-2). See the function docs for the refinement sweep.
    3e-2
}

// ===========================================================================
// Phase-2.5C (Epic #318): order-aware (p=2) dielectric eigensolver
// ===========================================================================

/// Curl-energy floor separating the physical guided band from the
/// gradient-nullspace band for the **p=2** Nédélec pencil.
///
/// # Why p=2 needs its own floor
///
/// The p=1 floor [`physical_curl_floor`] (`3e-2`) is calibrated against the
/// measured curl-energy gap of the *Whitney* pencil. At p=2 the pencil's
/// gradient nullspace is larger (it gains the `Q = ∇(λ_aλ_b)` edge DOFs and
/// interior modes — see [`spurious_dim_2d_p2`]), but those gradient modes
/// are still *exactly* curl-free by construction (`∇×Q ≡ 0`,
/// `∇×∇φ ≡ 0`): a converged p=2 gradient eigenpair has `r = (xᵀKx)/(k₀²xᵀM_εx)`
/// at the f64 noise floor (`≈ 10⁻¹⁶`), the same as p=1. What changes is the
/// *guided* band: the richer p=2 representation resolves the genuine mode's
/// curl with **more** curl energy per unit field energy, so the genuine
/// band floor moves *up*, not down — the gap can only widen, never narrow.
///
/// # The recalibration is algebraic, not a fitted constant
///
/// The robust discriminant is the **algebraic** generalized de-Rham
/// nullspace dimension [`spurious_dim_2d_p2`] = (interior nodes) +
/// (interior edges): every member of that nullspace is exactly curl-free
/// and lands at `r ≈ 0`, so any floor in `(noise, genuine-band-floor)`
/// separates them. A p=2 curl-energy refinement sweep (rect TE-cavity and
/// the SOI strip at ny ∈ {20,30,40,60}) shows the p=2 genuine guided band
/// floor at `r ≳ 1.2×10⁻¹` — *above* the p=1 genuine floor (`≈ 8.5×10⁻²`),
/// confirming the gap widens with order. The p=1 floor `3×10⁻²` therefore
/// already sits comfortably inside the (wider) p=2 gap, so **reusing it is
/// safe**; we keep an explicit p=2 entry point (returning the same `3e-2`)
/// so the calibration is documented and order-local rather than implicitly
/// shared, and so a future p≥3 extension has an obvious hook. The floor is
/// never *raised* by an out-of-window spike (it is a fixed constant, not a
/// gap-widening rule), exactly as argued for p=1.
fn physical_curl_floor_p2() -> f64 {
    // Same numeric value as the p=1 floor: the p=2 gradient nullspace is
    // exactly curl-free (r ≈ 0) and the p=2 genuine guided band floor sits
    // *above* the p=1 one, so the p=1 gap (1.7e-2, 8.5e-2) is contained in
    // the wider p=2 gap and the centred 3e-2 floor remains valid. Kept as a
    // distinct symbol so the p=2 calibration is explicit (Epic #318 2.5C).
    3e-2
}

/// Order-aware (p=2) sibling of [`solve_dielectric_modes`]: solve the
/// dielectric full-vector transverse-mode eigenproblem using the
/// **second-order** Nédélec assembly ([`assemble_2d_nedelec2_with_epsilon`])
/// and the p=2 interior-DOF mask (e.g. [`rect_pec_interior_dofs2`] /
/// [`disk_pec_interior_dofs2`]).
///
/// The eigenpencil `A = k₀²M_ε − K`, `A x = β² M₁ x`, the sparse
/// shift-invert Lanczos path, the guided-band shift target, the
/// bound-window classifier, the [`physical_index_ceiling`] geometry ceiling,
/// and the [`pin_eigenvector_sign`] gauge are all **order-agnostic** and
/// reused verbatim; only the assembly order and the curl-energy floor
/// ([`physical_curl_floor_p2`]) differ. Returns up to `n_modes` guided
/// [`DielectricMode`]s ordered fundamental-first (largest `n_eff`), with
/// `e_edges` in the **p=2 DOF ordering** (length [`n_dof_2d_nedelec2`]).
///
/// The p=1 [`solve_dielectric_modes`] path is left bit-for-bit unchanged.
///
/// # Errors
///
/// Returns [`EigenError`] if the sparse eigensolve fails. Returns an empty
/// `Vec` (not an error) if no bound modes exist in the window.
pub fn solve_dielectric_modes2(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    let index_ceiling = physical_index_ceiling(mesh, eps_r, k0);
    let n_eff_ceiling = index_ceiling.unwrap_or(n_core);
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let n_request = (n_modes + 8).max(16);
    let cands =
        dielectric_raw_candidates_p2(mesh, eps_r, interior_dof_mask, k0, n_request, index_ceiling)?;
    if cands.is_empty() {
        return Ok(Vec::new());
    }
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_p2();

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
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
        let mut e_edges = vec![0.0_f64; n_dof];
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

    bound.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bound.truncate(n_modes);
    let have = bound.len();
    let ceiling_kind = if index_ceiling.is_some() {
        "physical-2D-slab"
    } else {
        "n_core"
    };
    eprintln!(
        "solve_dielectric_modes2 (p=2): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} ({ceiling_kind}); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} bound mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s) (requested {n_modes})"
    );
    Ok(bound)
}

// ===========================================================================
// Epic #303 PML-B (#332): complex-pencil PML dielectric modal solve
// ===========================================================================

/// A single guided / leaky transverse mode of a **PML-terminated**
/// dielectric waveguide cross-section, the complex-pencil analogue of
/// [`DielectricMode`] (Epic #303 PML-B, issue #332).
///
/// With the cladding absorbed by a 2D UPML (instead of truncated by a far
/// PEC wall), the modal pencil `A = k₀² M_ε − K`, `A x = β² M₁ x` is
/// **complex-symmetric**: the eigenvalue `β²` acquires a small imaginary
/// part. A genuinely bound, low-loss mode sits near the real axis
/// (`|Im(β²)|` small); a radiating/leaky one has large `|Im(β²)|`. The
/// effective index `n_eff = √(β²)/k₀` is therefore complex —
/// `Re(n_eff)` is the propagating effective index and `Im(n_eff)` the
/// modal loss / leakage rate (negative imaginary part ⇒ decaying mode).
#[derive(Debug, Clone)]
pub struct DielectricModePml {
    /// Complex effective index `n_eff = √(β²)/k₀` (principal branch,
    /// `Re ≥ 0`). `Re` is the propagating effective index; `Im` is the
    /// modal loss/leakage figure.
    pub n_eff: c64,
    /// Complex propagation constant `β = n_eff · k₀ = √(β²)`.
    pub beta: c64,
    /// Generalized-pencil eigenvalue `β²` (complex). The selection figure
    /// of merit is `|Im(β²)|` (smallest ⇒ genuinely bound / lowest-loss).
    pub beta_sq: c64,
    /// `true` if this mode is **guided**: `Re(β²)` lies in the index
    /// window `(n_clad² k₀², n_eff_ceiling² k₀²)`, it carries curl energy
    /// above the floor, and it is the smallest-`|Im(β²)|` such candidate.
    pub guided: bool,
    /// Full-length **complex** transverse field over the 2-D mesh p=2 DOFs
    /// (length [`n_dof_2d_nedelec2`]). PEC-eliminated DOFs carry exact
    /// zeros. Bilinear-M-normalized.
    pub e_edges: Vec<c64>,
}

/// A raw recovered eigenpair of the **complex** PML dielectric pencil
/// `A x = β² M₁ x` before window / curl-energy classification (the
/// complex analogue of [`RawDielectricCandidate`]).
struct RawDielectricCandidateComplex {
    beta_sq: c64,
    /// Relative curl energy `r = |xᴴ K x| / (k₀² |xᴴ M_ε x|)` of the
    /// eigenvector (≈ 0 for a gradient-nullspace mode, O(1) for a genuine
    /// guided/physical mode). Magnitudes are used so the figure is real
    /// and comparable to the real-path curl floor.
    curl_ratio: f64,
    /// Interior-DOF complex eigenvector (length = interior DOF count).
    vector: Vec<c64>,
}

/// The complex standard-form pencil `A = k₀² M_ε − K`, assembled from the
/// two interior-restricted complex operators (the `c64` analogue of
/// [`sparse_pencil_a`]).
fn sparse_pencil_a_c64(
    k: SparseColMatRef<'_, usize, c64>,
    m_eps: SparseColMatRef<'_, usize, c64>,
    k0_sq: f64,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let n = k.nrows();
    let nnz = k.col_ptr()[n] + m_eps.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(nnz);
    let push = |trips: &mut Vec<Triplet<usize, usize, c64>>,
                a: SparseColMatRef<'_, usize, c64>,
                scale: c64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for kk in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[kk], j, scale * v[kk]));
            }
        }
    };
    push(&mut trips, m_eps, c64::new(k0_sq, 0.0));
    push(&mut trips, k, c64::new(-1.0, 0.0));
    triplets_to_sparse_c64(n, &trips)
}

/// Hermitian quadratic form `xᴴ A x = Σ conj(x_i) (A x)_i` for a complex
/// sparse `A` and complex dense `x`. Used for the (real-valued) curl-energy
/// ratio of a complex mode — taking magnitudes keeps the figure comparable
/// to the real-path curl floor.
fn sparse_quadratic_form_c64_herm(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> c64 {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut acc = c64::new(0.0, 0.0);
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj.re == 0.0 && xj.im == 0.0 {
            continue;
        }
        for kk in cp[j]..cp[j + 1] {
            let i = ri[kk];
            // conj(x_i) * A[i,j] * x_j
            acc += x[i].conj() * v[kk] * xj;
        }
    }
    acc
}

/// Principal complex square root with `Re(√z) ≥ 0` — the `n_eff`/`β`
/// recovery branch for the complex PML pencil. (A local copy of the
/// branch used by the complex Lanczos, kept self-contained here.)
fn principal_sqrt_c64(z: c64) -> c64 {
    if z.re == 0.0 && z.im == 0.0 {
        return c64::new(0.0, 0.0);
    }
    let r = (z.re * z.re + z.im * z.im).sqrt();
    let re = ((r + z.re) * 0.5).sqrt();
    let im_mag = ((r - z.re) * 0.5).sqrt();
    let im = if z.im >= 0.0 { im_mag } else { -im_mag };
    c64::new(re, im)
}

/// Complex p=2 PML analogue of [`dielectric_raw_candidates_p2`]: assemble
/// the complex UPML pencil `A = k₀² M_ε − K`, `M₁` from
/// [`assemble_2d_nedelec2_pml_sparse_interior`], and recover up to
/// `n_request` raw eigenpairs (`β²`, relative curl energy, complex
/// eigenvector) via [`SparseComplexShiftInvertLanczos`] — the **same
/// complex bilinear-Lanczos path the Mie pencil uses**. The real shift `σ`
/// is placed just below the guided-band ceiling (in-window `β²`), since the
/// guided/low-loss eigenvalues sit near the real axis.
#[allow(clippy::too_many_arguments)]
fn dielectric_raw_candidates_p2_pml(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_request: usize,
    n_eff_target: Option<f64>,
) -> Result<Vec<RawDielectricCandidateComplex>, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec2_pml_sparse_interior(
        mesh,
        eps_r,
        region_tags,
        interior_dof_mask,
        r_pml_inner,
        r_outer,
        sigma_0,
    )?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_int = ops.k;
    let m_eps_int = ops.m_eps;
    let m1_sparse = ops.m1;

    let a_sparse = sparse_pencil_a_c64(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;

    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    // Real shift just under the guided-band ceiling — guided eigenvalues
    // sit near the real axis, so a real σ keeps the K − σM LU cheap and
    // still targets the band (see complex_lanczos.rs σ discussion).
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

    let curl_ratio = |x: &[c64]| -> f64 {
        let xkx = sparse_quadratic_form_c64_herm(k_int.as_ref(), x).norm();
        let xmx = sparse_quadratic_form_c64_herm(m_eps_int.as_ref(), x).norm();
        let denom = (k0_sq * xmx).max(1e-300);
        xkx / denom
    };

    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseComplexShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
    };
    let pairs = solver.smallest_eigenpairs(a_sparse.as_ref(), m1_sparse.as_ref(), n_req)?;

    let mut cands: Vec<RawDielectricCandidateComplex> = pairs
        .iter()
        .map(|pair| RawDielectricCandidateComplex {
            beta_sq: pair.lambda,
            curl_ratio: curl_ratio(&pair.vector),
            vector: pair.vector.clone(),
        })
        .collect();
    // Sort by decreasing Re(β²) (highest-index first) for stable logging.
    cands.sort_by(|a, b| {
        b.beta_sq
            .re
            .partial_cmp(&a.beta_sq.re)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(cands)
}

/// Curl-energy floor for the **PML** dielectric path (Epic #303 PML-B,
/// issue #332).
///
/// This is deliberately much smaller than the PEC-path
/// [`physical_curl_floor_p2`] (`3e-2`). That floor was calibrated for a
/// **high-contrast** strip on a PEC-walled domain, where the genuine
/// guided band floors at `r ≈ 8.5×10⁻²`. A **weakly-guiding** fiber
/// (SMF-28, Δ ≈ 0.4 %, V = 2.135) has an almost-TEM fundamental whose
/// relative curl energy `r = |xᴴ K x|/(k₀²|xᴴ M_ε x|)` is intrinsically
/// **tiny** — empirically `r ≈ 10⁻⁴…10⁻²` for the genuine guided modes,
/// while the curl-free gradient-nullspace cluster sits at `r ≈ 10⁻¹³` (to
/// f64 noise). The two populations are separated by ~9 orders of
/// magnitude, so a floor placed in the gap (`10⁻⁶`) cleanly rejects the
/// gradient nullspace while keeping every physical guided/leaky mode. The
/// smallest-`|Im(β²)|`-in-window rule then selects the genuine LP₀₁ among
/// the survivors (confirmed by core-energy fraction ≳0.8).
fn physical_curl_floor_pml() -> f64 {
    1e-6
}

/// PML / complex-pencil sibling of [`solve_dielectric_modes2`] (Epic #303
/// PML-B, issue #332). Solve the dielectric full-vector transverse-mode
/// eigenproblem on a **PML-terminated** cross-section (a
/// [`disk_tri_mesh_pml`] mesh with the cladding absorbed by a 2D UPML),
/// returning up to `n_modes` [`DielectricModePml`] guided modes.
///
/// # The complex pencil and the clean selection
///
/// With the UPML weights the modal pencil
///
/// ```text
///   A x = β² M₁ x,   A = k₀² M_ε − K   (all complex c64),
/// ```
///
/// is **complex-symmetric** (bilinear, not Hermitian), so it is solved by
/// [`SparseComplexShiftInvertLanczos`] — the exact path the Mie open-cavity
/// pencil uses. A real shift `σ` just below the guided-band ceiling targets
/// the band (guided eigenvalues sit near the real axis).
///
/// Because the PML absorbs the cladding, the box / cladding-resonance modes
/// that polluted the PEC-walled guided window (issue #329) are gone — they
/// are pushed to large `|Im(β²)|` (lossy/radiating). The genuine guided
/// LP₀₁ is therefore the eigenpair with the **smallest `|Im(β²)|`**
/// (genuinely bound, lowest loss) whose `Re(β²)` lies inside the index
/// window `(n_clad² k₀², n_eff_ceiling² k₀²)` and which carries curl energy
/// above [`physical_curl_floor_p2`]. The core-energy fraction
/// ([`dielectric_mode_field_shape_pml`]) then **confirms** the selection
/// (≳0.8 for a genuine LP₀₁) rather than driving it.
///
/// # σ₀ reduction
///
/// With `sigma_0 = 0` the complex assembly reduces bit-for-bit to the real
/// path embedded in `c64`, so this returns the same guided mode as the real
/// [`solve_dielectric_modes2`] (now with `Im(β²) ≈ 0`).
///
/// # Arguments
///
/// - `mesh` / `region_tags` — from [`disk_tri_mesh_pml`].
/// - `eps_r` — per-triangle ε_r (length `mesh.n_tris()`).
/// - `interior_dof_mask` — p=2 PEC mask (e.g. [`disk_pec_interior_dofs2`]).
/// - `r_pml_inner` / `r_outer` — PML annulus radii (`cladding_outer` and
///   `outer_radius` of the mesh).
/// - `sigma_0` — UPML strength (> 0 turns on absorption).
/// - `k0` — optical free-space wavenumber `2π/λ` (> 0).
/// - `n_modes` — maximum guided modes to return (fundamental first).
///
/// # Errors
///
/// Returns [`EigenError`] if the complex eigensolve fails. Returns an empty
/// `Vec` (not an error) if no guided mode exists in the window.
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_pml(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricModePml>, EigenError> {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    // Use the **n_core** ceiling for the PML path — NOT the slab-strip
    // ceiling [`physical_index_ceiling`]. That strip ceiling was a PEC-era
    // crutch for high-contrast rectangular cores: it treats the core as a
    // 1-D slab and clips the near-`n_core` spurious cluster the PEC wall
    // manufactured. For a circular weakly-guiding fiber it *over*-clips —
    // the genuine LP₀₁ sits just above the slab-derived ceiling — and the
    // PML doesn't need it: boundness (`|Im(β²)| ≈ 0`) plus core
    // confinement already isolate the fundamental cleanly. So we keep the
    // full `(n_clad², n_core²) k₀²` window and let the smallest-|Im| /
    // lowest-order selection (confirmed by core-energy fraction) do the
    // work.
    let n_eff_ceiling = n_core;
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    // Request a generous batch: the in-window band is densely populated
    // (gradient nullspace + bound + leaky), and the genuine bound cluster
    // sits a little below the ceiling, so a small request can miss the
    // fundamental. 40 comfortably samples the whole guided window for the
    // SMF-28-scale meshes this targets.
    let n_request = (n_modes + 36).max(40);
    let cands = dielectric_raw_candidates_p2_pml(
        mesh,
        eps_r,
        region_tags,
        interior_dof_mask,
        r_pml_inner,
        r_outer,
        sigma_0,
        k0,
        n_request,
        None,
    )?;
    if cands.is_empty() {
        return Ok(Vec::new());
    }
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_pml();

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // Keep only in-window, curl-bearing candidates; select by SMALLEST
    // |Im(β²)| (genuinely bound / lowest leakage) — the clean PML selection.
    let mut guided: Vec<DielectricModePml> = Vec::new();
    let mut n_dropped = 0usize;
    for c in &cands {
        let in_window = c.beta_sq.re > beta_sq_floor && c.beta_sq.re < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        let beta = principal_sqrt_c64(c.beta_sq);
        let n_eff = beta / c64::new(k0, 0.0);
        let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        guided.push(DielectricModePml {
            n_eff,
            beta,
            beta_sq: c.beta_sq,
            guided: true,
            e_edges,
        });
    }

    // Clean PML selection — smallest |Im(β²)| (genuinely bound), then
    // lowest order (largest Re(β²)).
    //
    // With the cladding absorbed, the in-window curl-bearing survivors
    // split cleanly into two populations by **relative** leakage
    // `|Im(β²)| / Re(β²)`:
    //   - genuinely **bound** modes — leakage at f64 noise
    //     (`≈ 10⁻¹⁷…10⁻¹⁴`), the PML adds no spurious loss to a truly
    //     trapped mode; and
    //   - **leaky/radiating** modes — leakage `≈ 10⁻⁵…10⁻¹` (the
    //     box/cladding-resonance content the PML pushed off the real
    //     axis).
    // The gap spans ~7+ orders of magnitude, so a relative cut at `10⁻⁸`
    // partitions them robustly. Among the **bound** cluster, leakage is
    // all at noise — so |Im| alone can't order them; the genuine
    // fundamental LP₀₁ is the **lowest-order** bound mode, i.e. the
    // largest `Re(β²)` (highest n_eff, most confined). We therefore sort
    // bound-before-leaky, then by descending `Re(β²)` within the bound
    // cluster (and by ascending |Im| within the leaky tail). The
    // core-energy fraction then **confirms** the pick is a genuine LP₀₁.
    const BOUND_REL_IM: f64 = 1e-8;
    let is_bound = |m: &DielectricModePml| -> bool {
        m.beta_sq.im.abs() <= BOUND_REL_IM * m.beta_sq.re.abs().max(1.0)
    };
    guided.sort_by(|a, b| {
        let (ba, bb) = (is_bound(a), is_bound(b));
        // Bound modes first.
        match (ba, bb) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        if ba {
            // Both bound: lowest-order = largest Re(β²).
            b.beta_sq
                .re
                .partial_cmp(&a.beta_sq.re)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            // Both leaky: smallest |Im(β²)| (least lossy) first.
            a.beta_sq
                .im
                .abs()
                .partial_cmp(&b.beta_sq.im.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    guided.truncate(n_modes);
    let have = guided.len();
    eprintln!(
        "solve_dielectric_modes2_pml (p=2, σ₀={sigma_0:.3}): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} (n_core); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} guided mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s) (requested {n_modes})"
    );
    Ok(guided)
}

/// Core-energy-fraction field-shape diagnostic of a [`DielectricModePml`]
/// (the complex-field analogue of [`dielectric_mode_field_shape`]). The
/// energy integrand is `|E|² = |Eₓ|² + |E_y|²` (complex squared
/// magnitudes), split into core (`tag == REGION_CORE`) and total buckets.
/// Used to **confirm** the PML-selected mode is a genuine LP₀₁ (core
/// fraction ≳0.8).
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()` or if `mode.e_edges` is
/// not the p=2 DOF length for `mesh`.
pub fn dielectric_mode_field_shape_pml(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricModePml,
) -> ModeFieldShape {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        mode.e_edges.len(),
        n_dof,
        "mode.e_edges length ({}) must equal p=2 DOF count ({})",
        mode.e_edges.len(),
        n_dof
    );

    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut core_energy = 0.0_f64;
    let mut total_energy = 0.0_f64;

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        let area_abs = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        let mut coef = [c64::new(0.0, 0.0); 8];
        for (i, item) in coef.iter_mut().enumerate() {
            let (gi, si) = dofs[i];
            *item = c64::new(si, 0.0) * mode.e_edges[gi];
        }

        let mut tri_energy = 0.0_f64;
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = c64::new(0.0, 0.0);
            let mut ey = c64::new(0.0, 0.0);
            for k in 0..8 {
                ex += coef[k] * c64::new(vals[k][0], 0.0);
                ey += coef[k] * c64::new(vals[k][1], 0.0);
            }
            tri_energy += w * (ex.norm() * ex.norm() + ey.norm() * ey.norm());
        }

        total_energy += tri_energy;
        if tag == REGION_CORE {
            core_energy += tri_energy;
        }
    }

    let core_energy_fraction = if total_energy > 0.0 {
        core_energy / total_energy
    } else {
        0.0
    };
    ModeFieldShape {
        core_energy_fraction,
        total_energy,
        core_energy,
    }
}

/// Field-shape diagnostics of a single recovered [`DielectricMode`],
/// computed from its p=2 edge-DOF profile on the disk mesh — used to
/// **identify the genuine fundamental LP₀₁** among the returned modes by
/// physical signature rather than by β-ordering alone.
///
/// The genuine LP₀₁ of a step-index fiber is **core-confined**: a high
/// fraction of its transverse-field energy `∫|E|²` lies inside the core
/// (`r < core_radius`), decaying evanescently into the cladding. PEC-box /
/// cladding-resonance modes that pollute the thin weakly-guiding window
/// instead oscillate/peak out in the cladding near the far wall and carry a
/// **low** core-energy fraction. Selecting the mode with the dominant
/// core-energy fraction therefore recovers the true LP₀₁ independent of
/// which box mode happens to land nearest the β ceiling.
#[derive(Clone, Copy, Debug)]
pub struct ModeFieldShape {
    /// `∫_core |E|² / ∫_total |E|²` — the core-energy fraction. LP₀₁ is
    /// dominant (high); box/cladding modes are low.
    pub core_energy_fraction: f64,
    /// Total field energy `∫_Ω |E|²` over the whole cross-section.
    pub total_energy: f64,
    /// Core field energy `∫_core |E|²` (`r < core_radius`).
    pub core_energy: f64,
}

/// Evaluate the 8 p=2 vector basis functions at a barycentric point — the
/// field-evaluation companion to [`tri_nedelec2_local`]'s internal `eval`
/// (kept in sync with it). Returns the basis **values** only (curls are not
/// needed for energy integration), in local DOF order
/// `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]`.
fn tri_nedelec2_basis_values(coords: &[[f64; 2]; 3], lam: [f64; 3]) -> [[f64; 2]; 8] {
    let det = {
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        e1[0] * e2[1] - e1[1] * e2[0]
    };
    let g = [
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
    let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
    let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
    };
    let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
    };
    let w0 = whitney(0, 1, l0, l1);
    let w1 = whitney(0, 2, l0, l2);
    let w2 = whitney(1, 2, l1, l2);
    let q0 = qgrad(0, 1, l0, l1);
    let q1 = qgrad(0, 2, l0, l2);
    let q2 = qgrad(1, 2, l1, l2);
    // I₀ = λ₂ W₀, I₁ = λ₀ W₂.
    let i0 = [l2 * w0[0], l2 * w0[1]];
    let i1 = [l0 * w2[0], l0 * w2[1]];
    [w0, q0, w1, q1, w2, q2, i0, i1]
}

/// Compute the [`ModeFieldShape`] (core-energy fraction) of a recovered p=2
/// dielectric mode on a disk mesh, splitting the energy integral by the
/// `disk_tri_mesh` region tags (tag `1` = core, anything else = cladding).
///
/// The transverse field is reconstructed per triangle from the mode's
/// `e_edges` p=2 DOFs via [`tri_nedelec2_basis_values`] and the global
/// DOF map (the same numbering [`assemble_2d_nedelec2_with_epsilon`] uses),
/// then `|E|²` is integrated with the degree-4 quadrature
/// [`TRI_QUAD_DEG4`] and accumulated into core vs total buckets.
///
/// This is a **pure field-analysis diagnostic** — it touches none of the
/// solver/eigensolve/assembly physics; it only reads back the field the
/// solver already returned.
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()` or if `mode.e_edges` is
/// not the p=2 DOF length for `mesh`.
pub fn dielectric_mode_field_shape(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricMode,
) -> ModeFieldShape {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        mode.e_edges.len(),
        n_dof,
        "mode.e_edges length ({}) must equal p=2 DOF count ({})",
        mode.e_edges.len(),
        n_dof
    );

    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut core_energy = 0.0_f64;
    let mut total_energy = 0.0_f64;

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        let area_abs = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        // Local coefficients with orientation sign folded in.
        let mut coef = [0.0_f64; 8];
        for (i, item) in coef.iter_mut().enumerate() {
            let (gi, si) = dofs[i];
            *item = si * mode.e_edges[gi];
        }

        let mut tri_energy = 0.0_f64;
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = 0.0_f64;
            let mut ey = 0.0_f64;
            for k in 0..8 {
                ex += coef[k] * vals[k][0];
                ey += coef[k] * vals[k][1];
            }
            tri_energy += w * (ex * ex + ey * ey);
        }

        total_energy += tri_energy;
        if tag == 1 {
            core_energy += tri_energy;
        }
    }

    let core_energy_fraction = if total_energy > 0.0 {
        core_energy / total_energy
    } else {
        0.0
    };
    ModeFieldShape {
        core_energy_fraction,
        total_energy,
        core_energy,
    }
}

/// p=2 analogue of [`dielectric_raw_candidates_with_target`]: assemble the
/// p=2 pencil `A = k₀²M_ε − K`, `M₁`, PEC-reduce with the p=2 interior-DOF
/// mask, and recover up to `n_request` raw eigenpairs (β², relative curl
/// energy, eigenvector), sorted by decreasing β². No bound-window/curl
/// filtering. The shift placement (guided-band target vs `n_core`) matches
/// the p=1 core exactly — only the assembly order differs.
fn dielectric_raw_candidates_p2(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
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
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        interior_dof_mask.len(),
        n_dof,
        "interior_dof_mask length ({}) must match p=2 DOF count ({})",
        interior_dof_mask.len(),
        n_dof
    );

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    // Interior-restricted sparse p=2 operators, assembled directly from the
    // 8×8 local blocks (no dense N×N round-trip). Nonzeros equal the previous
    // dense `apply_pec_2d(&assemble_2d_nedelec2_with_epsilon …)` output entry
    // for entry.
    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, eps_r, interior_dof_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_int = ops.k;
    let m_eps_int = ops.m_eps;

    let a_sparse = sparse_pencil_a(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;
    let m1_sparse = ops.m1;

    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

    let curl_ratio = |x_interior: &[f64]| -> f64 {
        let xkx = sparse_quadratic_form(k_int.as_ref(), x_interior);
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
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

/// Solve the **p=2** metallic curl-curl transverse modal eigenproblem on a
/// PEC cross-section — the order-2 analogue of
/// [`solve_waveguide_modes_with_opts`], used for the metallic-cutoff
/// regression and the manufactured order-of-convergence gate (Epic #318
/// 2.5C). Assembles with [`assemble_2d_nedelec2_with_epsilon`] (uniform
/// ε ≡ 1), PEC-reduces with the p=2 interior-DOF mask, estimates the shift
/// via [`estimate_modal_shift`], and returns the lowest `n_modes` physical
/// (`λ = k_c² > threshold`) eigenvalues, smallest first.
///
/// Returns the bare eigenvalues `λ = k_c²` (the field profile is not needed
/// for the cutoff/convergence checks); spurious gradient modes (`λ ≈ 0`)
/// are filtered by the σ-relative threshold.
pub fn solve_rect_waveguide_modes2_cutoffs(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    n_modes: usize,
) -> Result<Vec<f64>, EigenError> {
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    // Interior-restricted sparse K and M (uniform ε ≡ 1), assembled directly.
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, &eps_ones, interior_dof_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_sparse = ops.k;
    let m_sparse = ops.m1;

    let spurious_dim_hint = dim.saturating_sub(n_modes).min(dim);
    let (sigma, _first_phys) = estimate_modal_shift(
        k_sparse.as_ref(),
        m_sparse.as_ref(),
        n_modes,
        spurious_dim_hint,
    )?;
    let threshold = 0.1 * sigma;

    let mut n_request = (n_modes + 8).min(dim);
    loop {
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
        let physical: Vec<f64> = pairs
            .into_iter()
            .filter(|p| p.lambda > threshold)
            .map(|p| p.lambda.max(0.0))
            .take(n_modes)
            .collect();
        if physical.len() == n_modes {
            return Ok(physical);
        }
        if n_request >= dim {
            return Err(EigenError::FaerGevd(format!(
                "p=2 metallic modal solve: only recovered {} of {n_modes} physical modes \
                 (threshold {threshold:.3e}, σ = {sigma:.3e})",
                physical.len()
            )));
        }
        n_request = (n_request * 2).min(dim);
    }
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

    /// The 6-point degree-4 rule integrates every barycentric monomial of
    /// total degree ≤ 4 exactly against the closed form on the reference
    /// triangle `∫_T λ₀^a λ₁^b λ₂^c dA = a! b! c! / (a+b+c+2)! · 2|T|`
    /// (with `|T| = 1/2` for the unit reference triangle, so the factor is
    /// `a! b! c! / (a+b+c+2)!`).
    #[test]
    fn tri_quad_deg4_integrates_polynomials_exactly() {
        // weights sum to 1 (normalised to element area).
        let wsum: f64 = TRI_QUAD_DEG4.iter().map(|r| r[3]).sum();
        assert!((wsum - 1.0).abs() < 1e-12, "weights must sum to 1: {wsum}");

        fn fact(n: u32) -> f64 {
            (1..=n).map(|k| k as f64).product::<f64>().max(1.0)
        }
        // closed form of ∫_T λ₀^a λ₁^b λ₂^c dA over the *reference* triangle
        // (area 1/2): a!b!c!/(a+b+c+2)! .
        let exact =
            |a: u32, b: u32, c: u32| -> f64 { fact(a) * fact(b) * fact(c) / fact(a + b + c + 2) };

        let ref_area = 0.5_f64;
        for a in 0..=4u32 {
            for b in 0..=(4 - a) {
                for c in 0..=(4 - a - b) {
                    if a + b + c > 4 {
                        continue;
                    }
                    let num: f64 = TRI_QUAD_DEG4
                        .iter()
                        .map(|r| {
                            r[3] * r[0].powi(a as i32) * r[1].powi(b as i32) * r[2].powi(c as i32)
                        })
                        .sum::<f64>()
                        * ref_area;
                    let want = exact(a, b, c);
                    assert!(
                        (num - want).abs() < 1e-13,
                        "deg-4 quad wrong for λ0^{a} λ1^{b} λ2^{c}: got {num}, want {want}"
                    );
                }
            }
        }
    }

    /// A degree-5 monomial is NOT integrated exactly (guards against an
    /// accidentally-too-strong rule masking a basis-degree mistake).
    #[test]
    fn tri_quad_deg4_misses_degree5() {
        let ref_area = 0.5_f64;
        // ∫ λ0^5 dA = 5!*0!*0!/7! = 1/42 over the reference triangle.
        let num: f64 = TRI_QUAD_DEG4
            .iter()
            .map(|r| r[3] * r[0].powi(5))
            .sum::<f64>()
            * ref_area;
        let want = 1.0 / 42.0;
        assert!(
            (num - want).abs() > 1e-6,
            "degree-4 rule should not be exact at degree 5"
        );
    }

    /// **p=1 subset (load-bearing):** the 3×3 sub-block of the p=2 `K`/`M`
    /// over the three Whitney (first-edge) DOFs `{0, 2, 4}` must equal the
    /// closed-form first-order `tri_nedelec_local` kernel.
    #[test]
    fn p2_whitney_subblock_matches_p1() {
        // A deliberately non-degenerate, non-reference triangle.
        let coords = [[0.3, -0.2], [1.7, 0.1], [0.6, 1.4]];
        let (k1, m1, a1) = tri_nedelec_local(&coords);
        let (k2, m2, a2) = tri_nedelec2_local(&coords);
        assert!((a1 - a2).abs() < 1e-14, "areas must match: {a1} vs {a2}");

        let whitney = [0usize, 2, 4];
        for (i, &gi) in whitney.iter().enumerate() {
            for (j, &gj) in whitney.iter().enumerate() {
                assert!(
                    (k2[gi][gj] - k1[i][j]).abs() < 1e-10,
                    "K subblock mismatch at ({i},{j}): {} vs {}",
                    k2[gi][gj],
                    k1[i][j]
                );
                assert!(
                    (m2[gi][gj] - m1[i][j]).abs() < 1e-10,
                    "M subblock mismatch at ({i},{j}): {} vs {}",
                    m2[gi][gj],
                    m1[i][j]
                );
            }
        }
    }

    /// `K` and `M` are symmetric to tight tolerance.
    #[test]
    fn p2_local_matrices_symmetric() {
        let coords = [[0.0, 0.0], [2.1, 0.3], [0.4, 1.9]];
        let (k, m, _) = tri_nedelec2_local(&coords);
        for i in 0..8 {
            for j in 0..8 {
                assert!(
                    (k[i][j] - k[j][i]).abs() < 1e-12,
                    "K not symmetric at ({i},{j})"
                );
                assert!(
                    (m[i][j] - m[j][i]).abs() < 1e-12,
                    "M not symmetric at ({i},{j})"
                );
            }
        }
    }

    /// Single-element sanity checks.
    ///
    /// 1. The gradient edge functions `Q = ∇(λ_a λ_b)` carry zero curl, so
    ///    their `K` diagonal entries (and any coefficient vector supported
    ///    only on the curl-free DOFs `{1, 3, 5}`) yield zero curl energy.
    /// 2. A constant-curl field is integrated correctly: the curl-energy
    ///    of a unit Whitney DOF equals `(∇×W)² · |T|`.
    #[test]
    fn p2_local_sanity_checks() {
        let coords = [[0.1, 0.0], [1.2, -0.1], [0.5, 1.3]];
        let (k, _m, area) = tri_nedelec2_local(&coords);
        let area_abs = area.abs();

        // (1) curl-free gradient DOFs → zero curl energy.
        for &q in &[1usize, 3, 5] {
            assert!(
                k[q][q].abs() < 1e-12,
                "gradient DOF {q} should have zero curl energy, got {}",
                k[q][q]
            );
        }
        // A mixed gradient-only coefficient vector also gives zero energy.
        let mut e = 0.0;
        let coeff = [0.0, 1.3, 0.0, -0.7, 0.0, 2.1, 0.0, 0.0];
        for i in 0..8 {
            for j in 0..8 {
                e += coeff[i] * k[i][j] * coeff[j];
            }
        }
        assert!(
            e.abs() < 1e-11,
            "gradient-only curl energy must vanish: {e}"
        );

        // (2) constant-curl check: ∇×W₀ = 2 (g0 × g1)_z is constant, so
        // ∫ (∇×W₀)² dA = (∇×W₀)² |T| = K[0][0].
        let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
            - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
        let g0 = [
            (coords[1][1] - coords[2][1]) / det,
            (coords[2][0] - coords[1][0]) / det,
        ];
        let g1 = [
            (coords[2][1] - coords[0][1]) / det,
            (coords[0][0] - coords[2][0]) / det,
        ];
        let curl_w0 = 2.0 * (g0[0] * g1[1] - g0[1] * g1[0]);
        let want = curl_w0 * curl_w0 * area_abs;
        assert!(
            (k[0][0] - want).abs() < 1e-12,
            "constant-curl integration wrong: K[0][0]={} want {want}",
            k[0][0]
        );
    }

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

    /// **Reference-integral gauge convention holds across refinements**
    /// (issue #300, replacing the issue-#262 argmax assertion).
    ///
    /// Every returned eigenvector is rotated so its projection onto the
    /// fixed reference field it selects is non-negative. Verified across
    /// two mesh resolutions (`nx = 10` and `nx = 16`) on a `2 × 0.8`
    /// cross-section: for **each** mode the *same* reference (lowest index
    /// clearing the relative floor) is selected at both resolutions and
    /// the resulting projection is positive at both — i.e. the gauge sign
    /// is reproducible across meshes. PR #261's historical failure was
    /// that `nx = 10 → nx = 16` flipped the raw Lanczos sign on the
    /// x-antisymmetric TE₂₀ eigenvector (whose largest-magnitude DOF is
    /// not mesh-stable); the reference-integral gauge fixes exactly that
    /// case because TE₂₀ locks onto the `sin(2πx/a)` reference instead of
    /// a single DOF.
    ///
    /// The cross-section is deliberately **non-degenerate**: a `2 × 1`
    /// guide makes TE₂₀ (`k_c = 2π/a`) and TE₀₁ (`k_c = π/b`) degenerate,
    /// so the second eigenvector would be an arbitrary, genuinely
    /// mesh-dependent mixture of the two — a case with no well-defined
    /// per-mode gauge. `b = 0.8` separates them (`k_c` ≈ π vs 3.93).
    #[test]
    fn reference_gauge_convention_holds_across_refinements() {
        let (a, b) = (2.0_f64, 0.8_f64);
        // Reference fields mirroring those in `gauge_fix_eigenvector`.
        let refs: [fn(f64, f64) -> [f64; 2]; 6] = [
            |sx, _sy| [0.0, (std::f64::consts::PI * sx).sin()],
            |sx, _sy| [0.0, (2.0 * std::f64::consts::PI * sx).sin()],
            |_sx, sy| [(std::f64::consts::PI * sy).sin(), 0.0],
            |_sx, sy| [(2.0 * std::f64::consts::PI * sy).sin(), 0.0],
            |_sx, _sy| [1.0, 0.0],
            |_sx, _sy| [0.0, 1.0],
        ];
        // Returns (selected_reference_index, projection) for a mode.
        let select_ref = |mesh: &TriMesh, e: &[f64]| -> (usize, f64) {
            let edges = mesh.edges();
            let (mut xmin, mut ymin) = (f64::INFINITY, f64::INFINITY);
            let (mut xmax, mut ymax) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in &mesh.nodes {
                xmin = xmin.min(p[0]);
                ymin = ymin.min(p[1]);
                xmax = xmax.max(p[0]);
                ymax = ymax.max(p[1]);
            }
            let lx = (xmax - xmin).max(f64::EPSILON);
            let ly = (ymax - ymin).max(f64::EPSILON);
            let e_scale = e
                .iter()
                .fold(0.0, |a, &x| a + x * x)
                .sqrt()
                .max(f64::EPSILON);
            for (ri, f) in refs.iter().enumerate() {
                let mut proj = 0.0;
                let mut rscale = 0.0;
                for (i, &ei) in e.iter().enumerate() {
                    if ei == 0.0 {
                        continue;
                    }
                    let [ea, eb] = edges[i];
                    let pa = mesh.nodes[ea as usize];
                    let pb = mesh.nodes[eb as usize];
                    let t = [pb[0] - pa[0], pb[1] - pa[1]];
                    let sx = (0.5 * (pa[0] + pb[0]) - xmin) / lx;
                    let sy = (0.5 * (pa[1] + pb[1]) - ymin) / ly;
                    let fv = f(sx, sy);
                    let r = fv[0] * t[0] + fv[1] * t[1];
                    proj += ei * r;
                    rscale += r * r;
                }
                let floor = 1e-6 * e_scale * rscale.sqrt();
                if proj.abs() > floor {
                    return (ri, proj);
                }
            }
            (usize::MAX, 0.0) // fell through to argmax fallback
        };

        let mut selected_by_mesh: Vec<Vec<usize>> = Vec::new();
        for &nx in &[10usize, 16usize] {
            let ny = nx / 2;
            let mesh = rect_tri_mesh(nx, ny, a, b);
            let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
            assert_eq!(modes.len(), 2);
            let mut sel = Vec::new();
            for (i, mode) in modes.iter().enumerate() {
                let (ri, proj) = select_ref(&mesh, &mode.e_edges);
                eprintln!("nx={nx}: mode[{i}] selected reference {ri}, projection = {proj:+.6e}");
                assert_ne!(
                    ri,
                    usize::MAX,
                    "nx={nx} mode[{i}] matched no reference (fell through to argmax \
                     fallback) — gauge is not mesh-stable for this mode"
                );
                assert!(
                    proj > 0.0,
                    "reference-gauge violated: nx={nx} mode[{i}] projection onto its \
                     selected reference {ri} is {proj:+.6e}, must be > 0"
                );
                sel.push(ri);
            }
            selected_by_mesh.push(sel);
        }
        // The same reference must be selected for each mode at both
        // resolutions (this is what makes the gauge sign cross-mesh
        // reproducible).
        assert_eq!(
            selected_by_mesh[0], selected_by_mesh[1],
            "reference selection differed across meshes ({:?} vs {:?}); gauge sign \
             would not be cross-mesh reproducible",
            selected_by_mesh[0], selected_by_mesh[1]
        );
    }

    /// **Reference-integral gauge — M-orthonormality preserved** (issue
    /// #300). The gauge rotation is a unit-modulus (±1) scalar per mode,
    /// so it must leave the set-wise Gram `e_iᵀ M e_j = δ_ij` exactly
    /// intact. This is the norm/orthogonality invariant the issue
    /// requires unit-testing.
    #[test]
    fn reference_gauge_preserves_orthonormality() {
        let (a, b) = (2.0_f64, 1.0_f64);
        let mesh = rect_tri_mesh(16, 8, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 3).expect("multi-mode solve K=3");
        assert_eq!(modes.len(), 3);

        let (_k, m_dense) = assemble_2d_nedelec(&mesh);
        let n_edges = m_dense.nrows();

        let dot_me = |i: usize, j: usize| -> f64 {
            let mut acc = 0.0_f64;
            for p in 0..n_edges {
                for q in 0..n_edges {
                    acc += modes[i].e_edges[p] * m_dense[(p, q)] * modes[j].e_edges[q];
                }
            }
            acc
        };

        let tol = 1e-12_f64;
        for i in 0..modes.len() {
            for j in 0..modes.len() {
                let g = dot_me(i, j);
                let expect = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (g - expect).abs() < tol,
                    "gauge broke M-orthonormality: G[{i}][{j}] = {g} ≠ {expect}"
                );
            }
        }
    }

    /// **Reference-integral gauge helper unit test** (issue #300):
    /// verifies the in-place behaviour of [`gauge_fix_eigenvector`] on a
    /// tiny explicit mesh.
    ///
    /// 1. A vector with positive reference projection is left unchanged.
    /// 2. Its negation is flipped back (the gauge pins proj ≥ 0).
    /// 3. The flip is norm-preserving (Σ eᵢ² unchanged).
    /// 4. An all-zero vector falls through to the fallback without panic.
    #[test]
    fn gauge_fix_eigenvector_unit() {
        // 1×1 single-quad mesh → 5 edges (4 boundary + 1 diagonal).
        let mesh = rect_tri_mesh(1, 1, 1.0, 1.0);
        let edges = mesh.edges();
        let n = edges.len();

        // Construct a synthetic field whose uniform-x projection is
        // positive: put a positive weight on the horizontal bottom edge
        // (nodes 0→1, tangent +x).
        let mut v = vec![0.0_f64; n];
        // Find the bottom horizontal edge (y=0 on both endpoints).
        let bottom = edges
            .iter()
            .position(|e| {
                mesh.nodes[e[0] as usize][1] == 0.0 && mesh.nodes[e[1] as usize][1] == 0.0
            })
            .expect("a horizontal bottom edge exists");
        v[bottom] = 1.0;
        let norm0 = v.iter().map(|x| x * x).sum::<f64>();

        // Already-positive projection → unchanged.
        let mut v_pos = v.clone();
        gauge_fix_eigenvector(&mesh, &edges, &mut v_pos);
        assert_eq!(
            v_pos, v,
            "positive-projection vector must be left unchanged"
        );

        // Negated → flipped back to positive projection.
        let mut v_neg: Vec<f64> = v.iter().map(|x| -x).collect();
        gauge_fix_eigenvector(&mesh, &edges, &mut v_neg);
        assert_eq!(
            v_neg, v,
            "negative-projection vector must be flipped to match"
        );

        // Norm preserved through the flip.
        let norm_after = v_neg.iter().map(|x| x * x).sum::<f64>();
        assert!(
            (norm_after - norm0).abs() < 1e-15,
            "gauge flip must preserve the eigenvector norm: {norm_after} ≠ {norm0}"
        );

        // All-zero vector → fallback, no panic, stays zero.
        let mut z = vec![0.0_f64; n];
        gauge_fix_eigenvector(&mesh, &edges, &mut z);
        assert!(z.iter().all(|&x| x == 0.0));
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

    /// **p=2 dielectric solve path** (Epic #318 Phase 2.5C): the
    /// order-aware `solve_dielectric_modes2` assembles the p=2 pencil with
    /// `assemble_2d_nedelec2_with_epsilon` + the p=2 interior-DOF mask and
    /// returns a guided fundamental in the bound window. On a slab-like
    /// fixture the p=2 fundamental `n_eff` matches the 1-D slab oracle, and
    /// at equal mesh density it is **at least as accurate** as the p=1
    /// solve (the higher-order element resolves the y-profile better). Mesh
    /// kept modest because the p=2 dense assembly is ~4× the p=1 system.
    #[test]
    fn p2_dielectric_solve_returns_guided_fundamental() {
        let n_core = 3.45_f64; // silicon-ish
        let n_clad = 1.45_f64; // oxide
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let d = 0.30_f64;
        let (w, h) = (0.20_f64, 3.0_f64);
        let (nx, ny) = (4usize, 24usize);
        let (mesh, eps_r, _interior_p1) =
            slab_fixture(nx, ny, w, h, d, n_core * n_core, n_clad * n_clad);

        // p=2 interior-DOF mask + p=2 solve.
        let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);
        let modes2 =
            solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1).expect("p=2 dielectric solve");
        assert!(
            !modes2.is_empty(),
            "p=2 solve must return at least the fundamental guided mode"
        );
        let m2 = &modes2[0];
        assert!(m2.guided, "p=2 fundamental must be flagged guided");
        assert!(
            m2.n_eff > n_clad && m2.n_eff < n_core,
            "p=2 n_eff {} outside (n_clad, n_core)",
            m2.n_eff
        );
        // Field profile is in the p=2 DOF layout.
        assert_eq!(m2.e_edges.len(), n_dof_2d_nedelec2(&mesh));

        // Compare to the p=1 solve on the identical mesh against the slab
        // oracle: the p=2 fundamental n_eff is at least as accurate.
        let (_e, interior_p1) = rect_pec_interior_edges(&mesh, w, h);
        let modes1 = solve_dielectric_modes(&mesh, &eps_r, &interior_p1, k0, 1)
            .expect("p=1 dielectric solve");
        assert!(!modes1.is_empty());
        let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);
        let err1 = (modes1[0].n_eff - n_eff_oracle).abs();
        let err2 = (m2.n_eff - n_eff_oracle).abs();
        eprintln!(
            "p=2 dielectric: n_eff p1 = {:.6} (err {:.2e}), p2 = {:.6} (err {:.2e}), \
             oracle = {n_eff_oracle:.6}",
            modes1[0].n_eff, err1, m2.n_eff, err2
        );
        assert!(
            err2 <= err1 + 1e-9,
            "p=2 fundamental n_eff error {err2:.3e} worse than p=1 {err1:.3e}"
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
        let floor = physical_curl_floor();
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

    // ---- Phase 2.5B: global p=2 DOF numbering + ε assembly + signs ----

    /// DOF count is exactly `2·n_edges + 2·n_tris`, and the assembled
    /// matrices are square at that size.
    #[test]
    fn p2_global_dof_count() {
        let mesh = rect_tri_mesh(3, 2, 1.0, 0.7);
        let n_edges = mesh.edges().len();
        let n_tris = mesh.n_tris();
        let expect = 2 * n_edges + 2 * n_tris;
        assert_eq!(n_dof_2d_nedelec2(&mesh), expect);

        let eps = vec![1.0; n_tris];
        let (k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
        assert_eq!(k.nrows(), expect);
        assert_eq!(k.ncols(), expect);
        assert_eq!(m.nrows(), expect);
        assert_eq!(m.ncols(), expect);
    }

    /// Global `K` and `M` are symmetric to tolerance (a non-uniform ε makes
    /// the test bite the sign bookkeeping, not just a uniform scale).
    #[test]
    fn p2_global_matrices_symmetric() {
        let mesh = rect_tri_mesh(3, 3, 1.3, 0.9);
        let eps: Vec<f64> = (0..mesh.n_tris())
            .map(|t| 1.0 + 0.5 * (t % 4) as f64)
            .collect();
        let (k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
        let n = k.nrows();
        for i in 0..n {
            for j in 0..n {
                assert!(
                    (k[(i, j)] - k[(j, i)]).abs() < 1e-10,
                    "K not symmetric at ({i},{j}): {} vs {}",
                    k[(i, j)],
                    k[(j, i)]
                );
                assert!(
                    (m[(i, j)] - m[(j, i)]).abs() < 1e-10,
                    "M not symmetric at ({i},{j}): {} vs {}",
                    m[(i, j)],
                    m[(j, i)]
                );
            }
        }
    }

    /// **p=1-subset check (load-bearing):** restricting the assembled p=2
    /// system to the Whitney (even-indexed edge) DOFs `{2·0, 2·1, …}`
    /// reproduces `assemble_2d_nedelec_with_epsilon` to tight tolerance.
    /// This validates both the DOF numbering and the per-DOF Whitney signs
    /// at the *global* (shared-edge) level.
    #[test]
    fn p2_global_p1_subset_matches() {
        let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
        let eps: Vec<f64> = (0..mesh.n_tris())
            .map(|t| 1.0 + 0.25 * (t % 3) as f64)
            .collect();

        let (k1, m1) = assemble_2d_nedelec_with_epsilon(&mesh, &eps);
        let (k2, m2) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

        let n_edges = mesh.edges().len();
        for e_i in 0..n_edges {
            let gi = 2 * e_i; // Whitney DOF of edge e_i in the p=2 numbering
            for e_j in 0..n_edges {
                let gj = 2 * e_j;
                assert!(
                    (k2[(gi, gj)] - k1[(e_i, e_j)]).abs() < 1e-10,
                    "K p1-subset mismatch at edges ({e_i},{e_j}): {} vs {}",
                    k2[(gi, gj)],
                    k1[(e_i, e_j)]
                );
                assert!(
                    (m2[(gi, gj)] - m1[(e_i, e_j)]).abs() < 1e-10,
                    "M p1-subset mismatch at edges ({e_i},{e_j}): {} vs {}",
                    m2[(gi, gj)],
                    m1[(e_i, e_j)]
                );
            }
        }
    }

    /// Materialize a sparse `SparseColMat` into a dense `Mat<f64>` for an
    /// entry-for-entry comparison against the dense assembler's output.
    fn sparse_to_dense(a: SparseColMatRef<'_, usize, f64>) -> Mat<f64> {
        let mut out = Mat::<f64>::zeros(a.nrows(), a.ncols());
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for k in cp[j]..cp[j + 1] {
                out[(ri[k], j)] += v[k];
            }
        }
        out
    }

    /// Assert two dense matrices are equal entry-for-entry to `tol`.
    fn assert_dense_eq(a: &Mat<f64>, b: &Mat<f64>, tol: f64, what: &str) {
        assert_eq!(a.nrows(), b.nrows(), "{what}: row count mismatch");
        assert_eq!(a.ncols(), b.ncols(), "{what}: col count mismatch");
        for i in 0..a.nrows() {
            for j in 0..a.ncols() {
                let d = (a[(i, j)] - b[(i, j)]).abs();
                assert!(
                    d < tol,
                    "{what}: entry ({i},{j}) mismatch {} vs {} (Δ={d:.3e})",
                    a[(i, j)],
                    b[(i, j)]
                );
            }
        }
    }

    /// **Issue #327 headline correctness gate:** the direct sparse
    /// interior-restricted assembly (`assemble_2d_nedelec_sparse_interior`,
    /// p=1) must equal the previous dense path
    /// `apply_pec_2d(&assemble_2d_nedelec_with_epsilon(…))` **entry for
    /// entry** for K, M_ε and M₁, on a small mesh with NON-uniform ε.
    #[test]
    fn sparse_interior_p1_matches_dense_nonuniform_eps() {
        let mesh = rect_tri_mesh(5, 4, 1.3, 0.9);
        // Non-uniform ε: three distinct values cycled across triangles.
        let eps: Vec<f64> = (0..mesh.n_tris())
            .map(|t| 1.0 + 0.37 * (t % 3) as f64 + 0.11 * (t % 5) as f64)
            .collect();
        let (_edges, interior) = rect_pec_interior_edges(&mesh, 1.3, 0.9);

        // Dense reference path.
        let (k_dense, m_eps_dense) = assemble_2d_nedelec_with_epsilon(&mesh, &eps);
        let eps_ones = vec![1.0_f64; mesh.n_tris()];
        let (_k1, m1_dense_full) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);
        let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
        let (_k1_int, m1_int_dense) = apply_pec_2d(&k_dense, &m1_dense_full, &interior);

        // Sparse-direct path.
        let ops = assemble_2d_nedelec_sparse_interior(&mesh, &eps, &interior).unwrap();
        assert_eq!(ops.dim, k_int_dense.nrows(), "interior dim mismatch (p=1)");

        assert_dense_eq(
            &sparse_to_dense(ops.k.as_ref()),
            &k_int_dense,
            1e-12,
            "K p1",
        );
        assert_dense_eq(
            &sparse_to_dense(ops.m_eps.as_ref()),
            &m_eps_int_dense,
            1e-12,
            "M_eps p1",
        );
        assert_dense_eq(
            &sparse_to_dense(ops.m1.as_ref()),
            &m1_int_dense,
            1e-12,
            "M1 p1",
        );
    }

    /// **Issue #327 headline correctness gate (p=2):** the direct sparse
    /// interior-restricted assembly (`assemble_2d_nedelec2_sparse_interior`)
    /// must equal `apply_pec_2d(&assemble_2d_nedelec2_with_epsilon(…))` entry
    /// for entry for K, M_ε and M₁, with NON-uniform ε and the p=2
    /// interior-DOF mask. This exercises the per-DOF Whitney orientation
    /// signs through the sparse scatter-add.
    #[test]
    fn sparse_interior_p2_matches_dense_nonuniform_eps() {
        let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
        let eps: Vec<f64> = (0..mesh.n_tris())
            .map(|t| 1.0 + 0.29 * (t % 4) as f64 + 0.13 * (t % 3) as f64)
            .collect();
        let interior = rect_pec_interior_dofs2(&mesh, 1.1, 0.8);

        let (k_dense, m_eps_dense) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
        let eps_ones = vec![1.0_f64; mesh.n_tris()];
        let (_k1, m1_dense_full) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps_ones);
        let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
        let (_k1_int, m1_int_dense) = apply_pec_2d(&k_dense, &m1_dense_full, &interior);

        let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps, &interior).unwrap();
        assert_eq!(ops.dim, k_int_dense.nrows(), "interior dim mismatch (p=2)");

        assert_dense_eq(
            &sparse_to_dense(ops.k.as_ref()),
            &k_int_dense,
            1e-12,
            "K p2",
        );
        assert_dense_eq(
            &sparse_to_dense(ops.m_eps.as_ref()),
            &m_eps_int_dense,
            1e-12,
            "M_eps p2",
        );
        assert_dense_eq(
            &sparse_to_dense(ops.m1.as_ref()),
            &m1_int_dense,
            1e-12,
            "M1 p2",
        );
    }

    /// The sparse pencil `A = k₀² M_ε − K` must equal the dense
    /// `k₀² M_ε,int − K_int` entry for entry (the operator the eigensolve
    /// actually consumes), p=2, non-uniform ε.
    #[test]
    fn sparse_pencil_a_matches_dense_p2() {
        let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
        let eps: Vec<f64> = (0..mesh.n_tris())
            .map(|t| 1.0 + 0.4 * (t % 3) as f64)
            .collect();
        let interior = rect_pec_interior_dofs2(&mesh, 1.1, 0.8);
        let k0 = 2.7_f64;
        let k0_sq = k0 * k0;

        let (k_dense, m_eps_dense) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
        let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
        let dim = k_int_dense.nrows();
        let mut a_dense = Mat::<f64>::zeros(dim, dim);
        for i in 0..dim {
            for j in 0..dim {
                a_dense[(i, j)] = k0_sq * m_eps_int_dense[(i, j)] - k_int_dense[(i, j)];
            }
        }

        let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps, &interior).unwrap();
        let a_sparse = sparse_pencil_a(ops.k.as_ref(), ops.m_eps.as_ref(), k0_sq).unwrap();
        assert_dense_eq(&sparse_to_dense(a_sparse.as_ref()), &a_dense, 1e-12, "A p2");
    }

    /// **Shared-edge sign consistency (the key orientation guard):** two
    /// triangles sharing one edge. The two triangles traverse the shared
    /// edge with opposite *local* orientation, so each contributes the
    /// Whitney function with a `tri_edges` sign of opposite parity. The
    /// per-DOF sign rule must make those contributions **reinforce** (not
    /// cancel) on the shared Whitney DOF's diagonal, while the even gradient
    /// `Q` and the interior bubbles are unaffected.
    ///
    /// We verify this structurally: assemble the two-triangle mesh, find the
    /// shared edge, and check (a) its Whitney diagonal `M` entry equals the
    /// sum of the two elements' local Whitney `M` diagonals (same magnitude,
    /// reinforcing — no spurious cancellation), and (b) the same for the
    /// gradient `Q` diagonal, which carries sign `+1` on both elements.
    #[test]
    fn p2_shared_edge_sign_consistency() {
        // Two CCW triangles sharing the diagonal edge (0)-(2) on the unit
        // square. T0 = [0,1,2] traverses the diagonal 0→2 (local edge
        // (0,2), global-aligned → sign +1). T1 = [2,3,0] (a CCW relabelling
        // of the upper triangle) traverses the diagonal 2→0 (local edge
        // (2,0), against the global a<b direction → sign -1). The two
        // triangles therefore touch the shared Whitney DOF with OPPOSITE
        // orientation parity — the real guard: the sign rule must make the
        // diagonal contributions reinforce (sign² = +1) rather than cancel.
        let mesh = TriMesh {
            nodes: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            tris: vec![[0, 1, 2], [2, 3, 0]],
        };
        let edges = mesh.edges();
        let tri_edges = mesh.tri_edges();
        let n_edges = edges.len();
        let eps = vec![1.0; mesh.n_tris()];
        let (_k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

        // Identify the global edge shared by both triangles: nodes {0,2}.
        let shared = edges
            .iter()
            .position(|e| *e == [0, 2] || *e == [2, 0])
            .expect("shared edge (0,2) must exist");

        // Confirm the two triangles really do touch it with opposite parity
        // (otherwise the cancellation guard is vacuous).
        let signs: Vec<i8> = tri_edges
            .iter()
            .map(|row| {
                row.iter()
                    .find(|&&(g, _)| g as usize == shared)
                    .map(|&(_, s)| s)
                    .expect("triangle touches shared edge")
            })
            .collect();
        assert_eq!(
            signs[0] * signs[1],
            -1,
            "test fixture must give opposite orientation parity on the shared edge"
        );

        // Local-Whitney / Q diagonal contributions from each triangle for
        // the shared edge, accumulated by hand to predict the global entry.
        let mut expect_w_diag = 0.0;
        let mut expect_q_diag = 0.0;
        for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
            // local edge index (within this triangle) that maps to `shared`
            let lk = row
                .iter()
                .position(|&(g, _)| g as usize == shared)
                .expect("each triangle touches the shared edge");
            let coords = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (_kl, ml, _a) = tri_nedelec2_local(&coords);
            // Whitney sign squares to +1 on the diagonal, so the diagonal
            // contribution is always positive and the two triangles ADD.
            expect_w_diag += ml[2 * lk][2 * lk];
            expect_q_diag += ml[2 * lk + 1][2 * lk + 1];
        }

        let gw = 2 * shared; // Whitney DOF
        let gq = 2 * shared + 1; // gradient DOF
        assert!(
            (m[(gw, gw)] - expect_w_diag).abs() < 1e-12,
            "shared-edge Whitney M diagonal: assembled {} vs expected {} \
             (sign cancellation/doubling bug)",
            m[(gw, gw)],
            expect_w_diag
        );
        assert!(
            (m[(gq, gq)] - expect_q_diag).abs() < 1e-12,
            "shared-edge gradient M diagonal: assembled {} vs expected {}",
            m[(gq, gq)],
            expect_q_diag
        );

        // The shared Whitney DOF must actually receive contributions from
        // BOTH triangles (guards against the orientation logic silently
        // routing one triangle elsewhere) — i.e. its diagonal exceeds either
        // single-element contribution.
        assert!(
            m[(gw, gw)] > 0.0 && expect_w_diag > 0.0,
            "shared Whitney DOF received no mass contribution"
        );
        let _ = n_edges;
    }

    /// **Gradient-`Q` orientation guard (issue #325):** the gradient edge
    /// functions `Q = ∇(λ_a λ_b)` are *even* (symmetric under `a ↔ b`), so
    /// they must scatter with sign `+1` regardless of the global edge
    /// orientation — they do NOT flip the way the Whitney functions do.
    ///
    /// The diagonal `Q–Q` checks in `p2_shared_edge_sign_consistency` cannot
    /// catch a wrong `Q` sign, because `sign² = +1` squares any sign bug away.
    /// The discriminating signal is a `Q–Q` **off-diagonal** entry across a
    /// shared edge, where the cross sign `sign_i · sign_j` does NOT square
    /// out. This test hand-accumulates every global `M` `Q–Q` entry with the
    /// gradient sign forced to `+1` (the orientation-independent rule) and
    /// asserts the assembler agrees. If the edge `esign` were wrongly applied
    /// to the `Q` DOFs, the two triangles meeting at the shared diagonal —
    /// which touch it with OPPOSITE orientation parity — would corrupt the
    /// shared `Q`'s off-diagonal couplings to the other edges' `Q` DOFs, and
    /// the hand reference (which uses `+1`) would disagree.
    ///
    /// Concretely guards against the mutation "apply `esign` to local DOFs
    /// `1, 3, 5`", which the full suite otherwise passed silently.
    #[test]
    fn p2_shared_edge_q_offdiagonal_orientation_invariant() {
        // Same two-triangle fixture as p2_shared_edge_sign_consistency: the
        // shared diagonal edge is traversed with opposite local orientation
        // by the two triangles, so a wrong Q sign rule WILL surface on a
        // Q–Q off-diagonal (cross sign does not square to +1).
        let mesh = TriMesh {
            nodes: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            tris: vec![[0, 1, 2], [2, 3, 0]],
        };
        let edges = mesh.edges();
        let tri_edges = mesh.tri_edges();
        let n_edges = edges.len();
        let eps = vec![1.0; mesh.n_tris()];
        let (_k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

        // Hand-accumulate the global Q–Q block with the orientation-INDEPENDENT
        // rule (gradient sign ≡ +1), exactly as the correct assembler should.
        let mut q_ref = Mat::<f64>::zeros(n_edges, n_edges);
        for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
            let coords = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (_kl, ml, _a) = tri_nedelec2_local(&coords);
            // Local gradient DOFs are the odd local indices 1, 3, 5, mapping
            // to global edges row[0..3]. Q carries sign +1 (no esign).
            for (ka, &(ga, _sa)) in row.iter().enumerate() {
                for (kb, &(gb, _sb)) in row.iter().enumerate() {
                    q_ref[(ga as usize, gb as usize)] += ml[2 * ka + 1][2 * kb + 1];
                }
            }
        }

        // The assembler's Q DOF for global edge e is global index 2e+1.
        // Every entry — diagonal AND off-diagonal — must match the +1 rule.
        // The off-diagonal entries are the load-bearing ones: they FAIL if
        // esign is wrongly applied to Q (cross sign ≠ +1 across the shared
        // edge), while the diagonal would pass either way.
        let mut checked_offdiag = 0usize;
        for e_i in 0..n_edges {
            for e_j in 0..n_edges {
                let gi = 2 * e_i + 1;
                let gj = 2 * e_j + 1;
                assert!(
                    (m[(gi, gj)] - q_ref[(e_i, e_j)]).abs() < 1e-12,
                    "Q–Q M entry (edges {e_i},{e_j}) is orientation-dependent: \
                     assembled {} vs +1-rule reference {} — gradient esign bug",
                    m[(gi, gj)],
                    q_ref[(e_i, e_j)]
                );
                if e_i != e_j && q_ref[(e_i, e_j)].abs() > 1e-12 {
                    checked_offdiag += 1;
                }
            }
        }

        // The guard is only meaningful if at least one nonzero Q–Q
        // off-diagonal was actually exercised (otherwise the assertion above
        // is vacuous and a Q-flip mutation could still slip through).
        assert!(
            checked_offdiag > 0,
            "fixture produced no nonzero Q–Q off-diagonal entries; \
             the orientation guard would be vacuous"
        );

        // Sharpen the guard at the shared edge specifically: its Q DOF must
        // couple (off-diagonal) to at least one other edge's Q DOF, and that
        // coupling must match the +1-rule reference. This is the exact entry
        // the Q-flip mutation corrupts.
        let shared = edges
            .iter()
            .position(|e| *e == [0, 2] || *e == [2, 0])
            .expect("shared edge (0,2) must exist");
        let gq_shared = 2 * shared + 1;
        let mut shared_offdiag_nonzero = false;
        for e_j in 0..n_edges {
            if e_j == shared {
                continue;
            }
            let gj = 2 * e_j + 1;
            if q_ref[(shared, e_j)].abs() > 1e-12 {
                shared_offdiag_nonzero = true;
                assert!(
                    (m[(gq_shared, gj)] - q_ref[(shared, e_j)]).abs() < 1e-12,
                    "shared-edge Q off-diagonal to edge {e_j} is \
                     orientation-dependent (gradient esign bug)"
                );
            }
        }
        assert!(
            shared_offdiag_nonzero,
            "shared-edge Q DOF has no nonzero off-diagonal coupling; \
             cannot discriminate a Q-flip mutation"
        );
    }

    /// p=2 interior mask for `rect_tri_mesh`: length matches the DOF count,
    /// every interior-bubble DOF is interior, wall-aligned edge DOFs are
    /// boundary, and the edge-DOF interior flags agree with the first-order
    /// `rect_pec_interior_edges` mask (both DOFs of an edge share its flag).
    #[test]
    fn p2_rect_interior_mask() {
        let (w, h) = (1.0, 0.6);
        let mesh = rect_tri_mesh(3, 2, w, h);
        let n_edges = mesh.edges().len();
        let n_tris = mesh.n_tris();
        let mask = rect_pec_interior_dofs2(&mesh, w, h);
        assert_eq!(mask.len(), 2 * n_edges + 2 * n_tris);

        let (_edges, edge_interior) = rect_pec_interior_edges(&mesh, w, h);
        for (e, &interior) in edge_interior.iter().enumerate() {
            assert_eq!(mask[2 * e], interior, "Whitney DOF of edge {e}");
            assert_eq!(mask[2 * e + 1], interior, "gradient DOF of edge {e}");
        }
        // All interior bubble DOFs are interior.
        for (d, &interior) in mask.iter().enumerate().skip(2 * n_edges) {
            assert!(interior, "interior bubble DOF {d} must be interior");
        }
        // At least one boundary edge exists (the rectangle has walls).
        assert!(
            edge_interior.iter().any(|&b| !b),
            "rectangle must have wall-aligned (boundary) edges"
        );
    }

    /// p=2 interior mask for `disk_tri_mesh`: same structural guarantees,
    /// agreeing with `disk_pec_interior_edges`.
    #[test]
    fn p2_disk_interior_mask() {
        let outer = 1.0;
        let (mesh, _tags) = disk_tri_mesh(0.4, outer, 2, 12);
        let n_edges = mesh.edges().len();
        let n_tris = mesh.n_tris();
        let mask = disk_pec_interior_dofs2(&mesh, outer);
        assert_eq!(mask.len(), 2 * n_edges + 2 * n_tris);

        let (_edges, edge_interior) = disk_pec_interior_edges(&mesh, outer);
        for (e, &interior) in edge_interior.iter().enumerate() {
            assert_eq!(mask[2 * e], interior, "Whitney DOF of edge {e}");
            assert_eq!(mask[2 * e + 1], interior, "gradient DOF of edge {e}");
        }
        for (d, &interior) in mask.iter().enumerate().skip(2 * n_edges) {
            assert!(interior, "interior bubble DOF {d} must be interior");
        }
        assert!(
            edge_interior.iter().any(|&b| !b),
            "disk must have far-wall (boundary) edges"
        );
    }

    // -------------------------------------------------------------------
    // Phase-2.5C (Epic #318): p=2 de-Rham nullspace + curl-free exactness
    // -------------------------------------------------------------------

    /// The local p=2 d⁰ projection is **exact**: the gradient of every
    /// order-2 scalar Lagrange basis function lies in the p=2 Nédélec edge
    /// space, so the L²-projection residual is ~machine zero. This is the
    /// `d⁰` exactness check demanded by Epic #318 (the first-kind order-2
    /// de-Rham sequence is exact by construction).
    #[test]
    fn p2_discrete_gradient_is_exact_in_edge_space() {
        // A single non-degenerate triangle (not the reference) to exercise
        // the affine Jacobian.
        let coords = [[0.3, 0.1], [1.7, 0.4], [0.6, 1.9]];
        let (_k, m_local, _area) = tri_nedelec2_local(&coords);

        let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
            - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
        let g = [
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
        let area_abs = 0.5 * det.abs();

        let eval_vecs = |lam: [f64; 3]| -> [[f64; 2]; 8] {
            let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
            let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
            };
            let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
            };
            let w0 = whitney(0, 1, l0, l1);
            let w1 = whitney(0, 2, l0, l2);
            let w2 = whitney(1, 2, l1, l2);
            let q0 = qgrad(0, 1, l0, l1);
            let q1 = qgrad(0, 2, l0, l2);
            let q2 = qgrad(1, 2, l1, l2);
            let i0 = [l2 * w0[0], l2 * w0[1]];
            let i1 = [l0 * w2[0], l0 * w2[1]];
            [w0, q0, w1, q1, w2, q2, i0, i1]
        };

        // RHS b_{i,s} = ∫ N_i · ∇φ_s.
        let mut rhs = [[0.0_f64; 6]; 8];
        for qrow in TRI_QUAD_DEG4.iter() {
            let lam = [qrow[0], qrow[1], qrow[2]];
            let w = qrow[3] * area_abs;
            let vecs = eval_vecs(lam);
            let sgrads = tri_scalar2_grads(&g, lam);
            for i in 0..8 {
                for s in 0..6 {
                    rhs[i][s] += w * (vecs[i][0] * sgrads[s][0] + vecs[i][1] * sgrads[s][1]);
                }
            }
        }
        let coeffs = solve_8x6(&m_local, &rhs);

        // Residual ‖∇φ_s − Σ_i c_i N_i‖²_L2 (computed from the mass form)
        // must be machine-zero for every scalar basis function.
        for s in 0..6 {
            // ∫ |∇φ_s|²  and  cross/self terms via quadrature.
            let mut res2 = 0.0_f64;
            for qrow in TRI_QUAD_DEG4.iter() {
                let lam = [qrow[0], qrow[1], qrow[2]];
                let w = qrow[3] * area_abs;
                let vecs = eval_vecs(lam);
                let sgrads = tri_scalar2_grads(&g, lam);
                let mut recon = [0.0_f64; 2];
                for i in 0..8 {
                    recon[0] += coeffs[i][s] * vecs[i][0];
                    recon[1] += coeffs[i][s] * vecs[i][1];
                }
                let dx = sgrads[s][0] - recon[0];
                let dy = sgrads[s][1] - recon[1];
                res2 += w * (dx * dx + dy * dy);
            }
            assert!(
                res2 < 1e-18,
                "∇φ_{s} not exactly in the p=2 edge space: L2 residual² = {res2:.3e}"
            );
        }
    }

    /// The generalized p=2 de-Rham nullspace dimension equals the number of
    /// **interior scalar-p2 DOFs** = (interior nodes) + (interior edges),
    /// pinned on a known small structured mesh. Compare the p=1 count,
    /// which is interior nodes alone.
    #[test]
    fn p2_spurious_dim_counts_interior_scalar_dofs() {
        let (nx, ny) = (4usize, 3usize);
        let (w, h) = (2.0_f64, 1.0_f64);
        let mesh = rect_tri_mesh(nx, ny, w, h);

        let (_edges, edge_interior) = rect_pec_interior_edges(&mesh, w, h);
        let node_interior = rect_pec_interior_nodes(&mesh, w, h);
        let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);

        let n_int_nodes = node_interior.iter().filter(|&&b| b).count();
        let n_int_edges = edge_interior.iter().filter(|&&b| b).count();

        let p1_dim = spurious_dim_2d(&mesh, &edge_interior, &node_interior);
        let p2_dim = spurious_dim_2d_p2(&mesh, &dof_mask, &node_interior, &edge_interior);

        // p=1: interior nodes only.
        assert_eq!(p1_dim, n_int_nodes, "p=1 nullspace = interior nodes");
        // p=2: interior nodes + interior edges (the new Q edge DOFs).
        assert_eq!(
            p2_dim,
            n_int_nodes + n_int_edges,
            "p=2 nullspace must equal interior nodes ({n_int_nodes}) + interior edges \
             ({n_int_edges}); got {p2_dim}"
        );
        // The p=2 nullspace is strictly larger than p=1 (interior edges > 0).
        assert!(
            p2_dim > p1_dim,
            "p=2 nullspace ({p2_dim}) must exceed p=1 ({p1_dim})"
        );
    }

    // ----- Epic #303 PML-A (issue #331): UPML stretch tensor + PML mesh + complex p=2 assembly -----

    /// Read a `c64` entry `(r, c)` from a `SparseColMat<usize, c64>`.
    fn c64_entry(a: &SparseColMat<usize, c64>, r: usize, c: usize) -> c64 {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        let mut acc = c64::new(0.0, 0.0);
        for k in cp[c]..cp[c + 1] {
            if ri[k] == r {
                acc += v[k];
            }
        }
        acc
    }

    /// Read an `f64` entry `(r, c)` from a `SparseColMat<usize, f64>`.
    fn f64_entry(a: &SparseColMat<usize, f64>, r: usize, c: usize) -> f64 {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        let mut acc = 0.0_f64;
        for k in cp[c]..cp[c + 1] {
            if ri[k] == r {
                acc += v[k];
            }
        }
        acc
    }

    /// PML-tagged disk mesh: valid CCW mesh, three-region tags correct by
    /// centroid radius, PML annulus is the outer band, sane area fractions.
    #[test]
    fn disk_tri_mesh_pml_three_region_tags() {
        let core_r = 1.0;
        let clad_r = 2.0; // = r_pml_inner
        let outer_r = 3.0;
        let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 4, 24);
        assert_eq!(tags.len(), mesh.n_tris());

        // Every triangle CCW (positive signed area).
        for tri in &mesh.tris {
            let c = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (_, _, sa) = tri_nedelec2_local(&c);
            assert!(sa > 0.0, "triangle must be CCW; got area {sa}");
        }

        // Region tag matches centroid-radius band exactly.
        let mut a_core = 0.0;
        let mut a_clad = 0.0;
        let mut a_pml = 0.0;
        for (tri, &tag) in mesh.tris.iter().zip(tags.iter()) {
            let p = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
            let cy = (p[0][1] + p[1][1] + p[2][1]) / 3.0;
            let r = (cx * cx + cy * cy).sqrt();
            let expect = if r < core_r {
                REGION_CORE
            } else if r < clad_r {
                REGION_CLADDING
            } else {
                REGION_PML
            };
            assert_eq!(tag, expect, "tag mismatch at r={r}");
            // area
            let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
            let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1]];
            let area = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();
            match tag {
                REGION_CORE => a_core += area,
                REGION_CLADDING => a_clad += area,
                REGION_PML => a_pml += area,
                _ => unreachable!(),
            }
        }
        // All three regions present.
        assert!(a_core > 0.0 && a_clad > 0.0 && a_pml > 0.0);
        // PML is the OUTER band: every PML centroid radius ≥ clad_r, and the
        // PML area is close to the exact annulus area π(outer² − clad²) (the
        // polygonal mesh slightly under-fills the circle).
        let exact_pml = std::f64::consts::PI * (outer_r * outer_r - clad_r * clad_r);
        let frac = a_pml / exact_pml;
        assert!(
            (0.90..=1.0).contains(&frac),
            "PML area fraction of exact annulus = {frac} (expected ~1)"
        );
    }

    /// Stretch tensor: identity for r ≤ r_pml_inner; in the annulus Λ_t and
    /// Λ_t⁻¹ are mutual inverses; the radial eigenvalue is 1/s and transverse
    /// is s; complex entries appear only in the PML.
    #[test]
    fn pml_stretch_tensor_2d_inverse_and_identity() {
        let r_in = 2.0;
        let r_out = 3.0;
        let sigma_0 = 5.0;

        // Interior point: identity, real, curl_weight = 1.
        let (lam, cw) = pml_stretch_tensor_2d([1.0, 0.5], r_in, r_out, sigma_0);
        assert_eq!(cw, c64::new(1.0, 0.0));
        assert_eq!(lam[0][0], c64::new(1.0, 0.0));
        assert_eq!(lam[1][1], c64::new(1.0, 0.0));
        assert_eq!(lam[0][1], c64::new(0.0, 0.0));
        assert_eq!(lam[1][0], c64::new(0.0, 0.0));

        // sigma_0 = 0 → identity even in the annulus.
        let (lam0, cw0) = pml_stretch_tensor_2d([2.5, 0.0], r_in, r_out, 0.0);
        assert_eq!(cw0, c64::new(1.0, 0.0));
        assert_eq!(lam0[0][0], c64::new(1.0, 0.0));
        assert_eq!(lam0[0][1], c64::new(0.0, 0.0));

        // Annulus point on +x axis (r̂ = x̂): Λ_t = diag(1/s, s); complex.
        let r = 2.5;
        let (lam_a, cw_a) = pml_stretch_tensor_2d([r, 0.0], r_in, r_out, sigma_0);
        let u = (r - r_in) / (r_out - r_in);
        let s = c64::new(1.0, -sigma_0 * u * u);
        let s_inv = c64::new(1.0, 0.0) / s;
        // radial (xx) eigenvalue = 1/s, transverse (yy) = s.
        let close = |a: c64, b: c64| (a - b).norm() < 1e-12;
        assert!(close(lam_a[0][0], s_inv), "Λ_xx should be 1/s");
        assert!(close(lam_a[1][1], s), "Λ_yy should be s");
        assert!(close(lam_a[0][1], c64::new(0.0, 0.0)));
        assert!(close(cw_a, s_inv), "curl weight should be 1/s");
        assert!(lam_a[0][0].im != 0.0, "complex in PML");

        // Inverse consistency at an off-axis annulus point: Λ_t · Λ_t⁻¹ = I.
        let (lam_b, _) = pml_stretch_tensor_2d([1.8, 1.8], r_in, r_out, sigma_0);
        // Build Λ_t⁻¹ analytically: same construction with s↔1/s.
        let rr = (1.8_f64 * 1.8 + 1.8 * 1.8).sqrt();
        let ub = ((rr - r_in) / (r_out - r_in)).clamp(0.0, 1.0);
        let sb = c64::new(1.0, -sigma_0 * ub * ub);
        let sb_inv = c64::new(1.0, 0.0) / sb;
        let rx = 1.8 / rr;
        let ry = 1.8 / rr;
        let coeff_inv = sb - sb_inv; // (s − 1/s) for the inverse tensor
        let lam_inv = [
            [
                sb_inv + coeff_inv * c64::new(rx * rx, 0.0),
                coeff_inv * c64::new(rx * ry, 0.0),
            ],
            [
                coeff_inv * c64::new(ry * rx, 0.0),
                sb_inv + coeff_inv * c64::new(ry * ry, 0.0),
            ],
        ];
        // product = Λ_t · Λ_inv
        #[allow(clippy::needless_range_loop)] // explicit i,j,k matrix-product indices
        for a in 0..2 {
            for b in 0..2 {
                let mut acc = c64::new(0.0, 0.0);
                for kk in 0..2 {
                    acc += lam_b[a][kk] * lam_inv[kk][b];
                }
                let want = if a == b {
                    c64::new(1.0, 0.0)
                } else {
                    c64::new(0.0, 0.0)
                };
                assert!(
                    (acc - want).norm() < 1e-10,
                    "Λ·Λ⁻¹ entry ({a},{b}) = {acc}, want {want}"
                );
            }
        }
    }

    /// LOAD-BEARING: with sigma_0 = 0 (and even with PML-tagged triangles
    /// present), the complex UPML assembly equals the real
    /// `assemble_2d_nedelec2_sparse_interior` output embedded in c64 with zero
    /// imaginary part — entry for entry. Proves the complex path does not
    /// corrupt the validated real assembly.
    #[test]
    fn pml_assembly_sigma0_reduces_to_real_bit_for_bit() {
        let core_r = 1.0;
        let clad_r = 2.0;
        let outer_r = 3.0;
        let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 3, 18);
        // Dielectric ε_r: core 2.1, cladding 1.0; PML carries cladding ε_r.
        let eps_r: Vec<f64> = tags
            .iter()
            .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
            .collect();
        let mask = disk_pec_interior_dofs2(&mesh, outer_r);

        let real = assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &mask).unwrap();
        // sigma_0 = 0 with PML tags PRESENT (so the PML branch executes but
        // produces identity tensors).
        let cplx = assemble_2d_nedelec2_pml_sparse_interior(
            &mesh, &eps_r, &tags, &mask, clad_r, outer_r, 0.0,
        )
        .unwrap();

        assert_eq!(cplx.dim, real.dim);
        let n = real.dim;
        for c in 0..n {
            for r in 0..n {
                for (cm, rm, name) in [
                    (&cplx.k, &real.k, "K"),
                    (&cplx.m_eps, &real.m_eps, "M_eps"),
                    (&cplx.m1, &real.m1, "M1"),
                ] {
                    let cv = c64_entry(cm, r, c);
                    let rv = f64_entry(rm, r, c);
                    assert_eq!(cv.im, 0.0, "{name}({r},{c}) imag must be 0, got {cv}");
                    assert_eq!(
                        cv.re, rv,
                        "{name}({r},{c}) real mismatch: complex {} vs real {rv}",
                        cv.re
                    );
                }
            }
        }
    }

    /// With sigma_0 > 0 the PML annulus carries complex entries; the
    /// non-PML (core/cladding) block stays real; and K, M_eps, M1 are all
    /// complex-SYMMETRIC (bilinear, not Hermitian) — A == Aᵀ.
    #[test]
    fn pml_assembly_complex_symmetric_and_localized() {
        let core_r = 1.0;
        let clad_r = 2.0;
        let outer_r = 3.0;
        let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 3, 18);
        let eps_r: Vec<f64> = tags
            .iter()
            .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
            .collect();
        let mask = disk_pec_interior_dofs2(&mesh, outer_r);

        let cplx = assemble_2d_nedelec2_pml_sparse_interior(
            &mesh, &eps_r, &tags, &mask, clad_r, outer_r, 8.0,
        )
        .unwrap();
        let n = cplx.dim;

        // Complex-symmetric: A(r,c) == A(c,r) for K, M_eps, M1.
        let mut any_complex = false;
        for c in 0..n {
            for r in 0..n {
                for a in [&cplx.k, &cplx.m_eps, &cplx.m1] {
                    let v = c64_entry(a, r, c);
                    let vt = c64_entry(a, c, r);
                    assert!(
                        (v - vt).norm() < 1e-12,
                        "complex-symmetry: ({r},{c})={v} vs ({c},{r})={vt}"
                    );
                    if v.im.abs() > 1e-14 {
                        any_complex = true;
                    }
                }
            }
        }
        assert!(
            any_complex,
            "sigma_0 > 0 must introduce complex entries from the PML annulus"
        );
    }

    // ---- PML-B (#332): complex-pencil modal solve + clean LP01 ----------

    /// Build the SMF-28-like PML problem: per-triangle ε_r (core n_core²,
    /// cladding+PML n_clad²), the PEC mask, and the geometry.
    #[allow(clippy::type_complexity)]
    fn smf28_pml_problem(
        clad_mult: f64,
        pml_mult: f64,
        n_radial: usize,
        n_angular: usize,
    ) -> (TriMesh, Vec<i32>, Vec<f64>, Vec<bool>, f64, f64, f64) {
        const N_CORE: f64 = 1.4504;
        const N_CLAD: f64 = 1.4447;
        const A_UM: f64 = 4.1;
        const LAMBDA_UM: f64 = 1.55;
        let clad_r = clad_mult * A_UM;
        let outer_r = pml_mult * A_UM;
        let (mesh, tags) = disk_tri_mesh_pml(A_UM, clad_r, outer_r, n_radial, n_angular);
        let eps_r = epsilon_r_from_region_tags(&tags, |t| {
            if t == REGION_CORE {
                N_CORE * N_CORE
            } else {
                N_CLAD * N_CLAD
            }
        });
        let mask = disk_pec_interior_dofs2(&mesh, outer_r);
        let k0 = 2.0 * std::f64::consts::PI / LAMBDA_UM;
        (mesh, tags, eps_r, mask, clad_r, outer_r, k0)
    }

    /// **The headline PML-B test (#332).** With the cladding absorbed by a
    /// 2D UPML, the genuine weakly-guiding LP₀₁ of the SMF-28 fiber
    /// isolates cleanly — the thing the far PEC wall could not do (#329,
    /// best core-energy fraction only 0.34–0.49). We assert the selected
    /// fundamental is genuinely core-confined (core-energy fraction ≳0.8),
    /// has `Re(n_eff)` inside the index window `(n_clad, n_core)`, and is
    /// genuinely bound (`|Im(β²)|` tiny). We do **not** yet assert ≤1% b
    /// convergence — that is PML-C's convergence study; here we only show
    /// the mode is cleanly isolated.
    #[test]
    fn pml_smf28_isolates_clean_lp01() {
        const N_CORE: f64 = 1.4504;
        const N_CLAD: f64 = 1.4447;
        let (mesh, tags, eps_r, mask, clad_r, outer_r, k0) = smf28_pml_problem(8.0, 11.0, 5, 60);
        let sigma_0 = 6.0;

        let modes = solve_dielectric_modes2_pml(
            &mesh, &eps_r, &tags, &mask, clad_r, outer_r, sigma_0, k0, 1,
        )
        .expect("PML modal solve must succeed");
        assert!(
            !modes.is_empty(),
            "PML solve must return a guided LP01 (got none)"
        );
        let m = &modes[0];

        // Re(n_eff) strictly inside the weakly-guiding window.
        assert!(
            m.n_eff.re > N_CLAD && m.n_eff.re < N_CORE,
            "Re(n_eff)={} must lie in ({N_CLAD}, {N_CORE})",
            m.n_eff.re
        );

        // Genuinely bound: |Im(β²)| negligible (the PML adds no spurious
        // loss to a truly trapped mode).
        let rel_im = m.beta_sq.im.abs() / m.beta_sq.re.abs().max(1.0);
        assert!(
            rel_im < 1e-6,
            "guided LP01 must be genuinely bound: |Im(β²)|/Re(β²)={rel_im:.3e} (β²={})",
            m.beta_sq
        );

        // The payoff: core-energy fraction confirms genuine LP01, NOT a box
        // mode. PEC-era best was 0.34–0.49; a clean fundamental is ≳0.8.
        let shape = dielectric_mode_field_shape_pml(&mesh, &tags, m);
        assert!(
            shape.core_energy_fraction >= 0.8,
            "core-energy fraction {:.4} must be ≳0.8 (clean LP01); PEC-era best was 0.34–0.49",
            shape.core_energy_fraction
        );

        eprintln!(
            "pml_smf28_isolates_clean_lp01: Re(n_eff)={:.6}, Im(n_eff)={:.3e}, \
             |Im(β²)|={:.3e}, core_energy_fraction={:.4}",
            m.n_eff.re,
            m.n_eff.im,
            m.beta_sq.im.abs(),
            shape.core_energy_fraction
        );
    }

    /// With `sigma_0 = 0` the complex PML assembly reduces bit-for-bit to
    /// the real path (proven in
    /// `pml_assembly_sigma0_reduces_to_real_bit_for_bit`), so the complex
    /// pencil eigensolve must reproduce the **real** dielectric spectrum
    /// embedded in `c64`: every recovered β² is real (`Im ≈ 0`), and the
    /// σ₀=0 PML-selected mode's β² must coincide with one of the real-path
    /// `dielectric_raw_candidates_p2` eigenvalues on the same mesh.
    ///
    /// (We compare against the *raw* real spectrum rather than the filtered
    /// `solve_dielectric_modes2` output: the two entry points apply
    /// different ceilings and selection rules — the real PEC path uses the
    /// strip-slab ceiling + largest-β, the PML path the n_core ceiling +
    /// lowest-order-bound — so their *selected* modes legitimately differ.
    /// The load-bearing claim is the spectral equivalence, which this
    /// checks directly.)
    #[test]
    fn pml_sigma0_matches_real_solve() {
        // A modest higher-contrast disk (uniform region map: core ε=2.1,
        // else 1.0).
        let core_r = 1.0;
        let clad_r = 2.0;
        let outer_r = 3.0;
        let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 4, 24);
        let eps_r: Vec<f64> = tags
            .iter()
            .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
            .collect();
        let mask = disk_pec_interior_dofs2(&mesh, outer_r);
        let k0 = 2.0;

        // Complex PML solve with sigma_0 = 0 (transparent layer).
        let pml_modes =
            solve_dielectric_modes2_pml(&mesh, &eps_r, &tags, &mask, clad_r, outer_r, 0.0, k0, 1)
                .expect("σ₀=0 PML solve must succeed");
        assert!(!pml_modes.is_empty(), "σ₀=0 PML solve must return a mode");
        let pm = &pml_modes[0];

        // (a) σ₀=0 ⇒ real β² (PML off ⇒ real spectrum embedded in c64).
        assert!(
            pm.beta_sq.im.abs() < 1e-6 * pm.beta_sq.re.abs().max(1.0),
            "σ₀=0 must give a real β²: Im={:.3e}",
            pm.beta_sq.im
        );

        // (b) The PML β² must coincide with a REAL-path eigenvalue — the
        // complex path reproduces the validated real spectrum.
        let real_cands = dielectric_raw_candidates_p2(&mesh, &eps_r, &mask, k0, 32, None)
            .expect("real raw candidates must succeed");
        let best = real_cands
            .iter()
            .map(|c| (c.beta_sq - pm.beta_sq.re).abs())
            .fold(f64::INFINITY, f64::min);
        let rel = best / pm.beta_sq.re.abs().max(1e-12);
        assert!(
            rel < 1e-6,
            "σ₀=0 PML β²={:.8} must match a real-path eigenvalue (closest abs err {best:.3e}, rel {rel:.3e})",
            pm.beta_sq.re
        );
    }

    // ===================================================================
    // Graded / non-uniform mesh strategies (issue #337)
    // ===================================================================

    /// Sorted unique ring radii of a concentric disk mesh: every distinct
    /// node radius, ascending (includes 0 for the center).
    fn ring_radii(mesh: &TriMesh) -> Vec<f64> {
        let mut rs: Vec<f64> = mesh
            .nodes
            .iter()
            .map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt())
            .collect();
        rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        rs.dedup_by(|a, b| (*a - *b).abs() < 1e-12 * (a.abs() + 1.0));
        rs
    }

    #[test]
    fn graded_uniform_reproduces_disk_tri_mesh_bit_for_bit() {
        // The load-bearing guarantee: Uniform grading == the original mesher,
        // node-for-node, tri-for-tri, tag-for-tag (exact f64 equality).
        for &(cr, or, nr, na) in &[
            (1.0, 3.0, 2, 8),
            (0.6, 2.5, 4, 24),
            (1.0, 10.0, 5, 31),
            (2.0, 7.0, 1, 5),
        ] {
            let (m0, t0) = disk_tri_mesh(cr, or, nr, na);
            let (m1, t1) = disk_tri_mesh_graded(
                cr,
                or,
                nr,
                na,
                RadialGrading::Uniform,
                RadialGrading::Uniform,
            );
            assert_eq!(m0.nodes, m1.nodes, "node coords must match bit-for-bit");
            assert_eq!(m0.tris, m1.tris, "triangles must match bit-for-bit");
            assert_eq!(t0, t1, "region tags must match bit-for-bit");
        }
    }

    #[test]
    fn graded_uniform_reproduces_disk_tri_mesh_pml_bit_for_bit() {
        for &(cr, cl, or, nr, na) in &[
            (1.0, 2.0, 3.0, 4, 24),
            (0.5, 1.7, 4.0, 3, 18),
            (1.0, 5.0, 10.0, 6, 37),
        ] {
            let (m0, t0) = disk_tri_mesh_pml(cr, cl, or, nr, na);
            let (m1, t1) = disk_tri_mesh_pml_graded(
                cr,
                cl,
                or,
                nr,
                na,
                RadialGrading::Uniform,
                RadialGrading::Uniform,
                RadialGrading::Uniform,
            );
            assert_eq!(m0.nodes, m1.nodes);
            assert_eq!(m0.tris, m1.tris);
            assert_eq!(t0, t1);
        }
    }

    #[test]
    fn graded_uniform_reproduces_rect_tri_mesh_bit_for_bit() {
        for &(nx, ny, w, h) in &[
            (3usize, 2usize, 1.0, 1.0),
            (5, 7, 2.0, 0.5),
            (1, 1, 3.0, 4.0),
        ] {
            let m0 = rect_tri_mesh(nx, ny, w, h);
            let m1 =
                rect_tri_mesh_graded(nx, ny, w, h, RadialGrading::Uniform, RadialGrading::Uniform);
            assert_eq!(m0.nodes, m1.nodes);
            assert_eq!(m0.tris, m1.tris);
        }
    }

    #[test]
    fn graded_meshes_are_valid_ccw_and_nondegenerate() {
        // Each strategy must yield a connected mesh of strictly-positive-area
        // CCW triangles. Reuse the existing connectivity/CCW machinery.
        let gradings = [
            RadialGrading::Uniform,
            RadialGrading::Geometric { ratio: 1.4 },
            RadialGrading::Geometric { ratio: 0.7 },
            RadialGrading::Linear { ratio: 2.5 },
            RadialGrading::InterfaceClustered { strength: 1.5 },
        ];
        for g in gradings {
            let (mesh, tags) = disk_tri_mesh_graded(1.0, 3.0, 5, 31, g, g);
            assert_eq!(tags.len(), mesh.n_tris());
            let mut min_area = f64::INFINITY;
            for t in &mesh.tris {
                let a = signed_area(&mesh, t);
                assert!(a > 0.0, "non-CCW triangle {t:?} (area {a}) under {g:?}");
                min_area = min_area.min(a);
            }
            assert!(min_area > 1e-12, "degenerate triangle under {g:?}");
            // Ring radii strictly increasing (no collapsed / out-of-order ring).
            let rs = ring_radii(&mesh);
            for w in rs.windows(2) {
                assert!(w[1] > w[0], "non-monotonic ring radii under {g:?}");
            }
            // A ring boundary still lands exactly on the core radius.
            assert!(
                rs.iter().any(|&r| (r - 1.0).abs() < 1e-9),
                "core-radius ring missing under {g:?}"
            );
        }
    }

    #[test]
    fn graded_region_tags_and_area_fraction_correct_under_grading() {
        // The core area fraction must still match π·a²/π·R² = (a/R)²
        // regardless of how rings are distributed within a region.
        let cr = 1.0_f64;
        let or = 3.0_f64;
        let expected = (cr / or).powi(2);
        for g in [
            RadialGrading::Geometric { ratio: 1.5 },
            RadialGrading::Linear { ratio: 0.5 },
            RadialGrading::InterfaceClustered { strength: 2.0 },
        ] {
            let (mesh, tags) = disk_tri_mesh_graded(cr, or, 8, 64, g, g);
            let mut a_core = 0.0;
            let mut a_tot = 0.0;
            for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
                let area = signed_area(&mesh, t).abs();
                a_tot += area;
                if tag == REGION_CORE {
                    a_core += area;
                }
                // Tag must agree with the centroid-radius band.
                let p = [
                    mesh.nodes[t[0] as usize],
                    mesh.nodes[t[1] as usize],
                    mesh.nodes[t[2] as usize],
                ];
                let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
                let cy = (p[0][1] + p[1][1] + p[2][1]) / 3.0;
                let r = (cx * cx + cy * cy).sqrt();
                let expect = if r < cr { REGION_CORE } else { REGION_CLADDING };
                assert_eq!(tag, expect, "tag/centroid mismatch under {g:?}");
            }
            let frac = a_core / a_tot;
            // Polygonal approximation to the circle ⇒ a few-% tolerance.
            assert!(
                (frac - expected).abs() < 0.02,
                "core area fraction {frac:.4} vs expected {expected:.4} under {g:?}"
            );
        }
    }

    #[test]
    fn graded_boundary_node_set_unchanged_under_grading() {
        // The outer boundary node set keys on geometry (r ≈ outer_radius),
        // so grading must not change it: still exactly the outer ring.
        let or = 3.0;
        for g in [
            RadialGrading::Geometric { ratio: 1.6 },
            RadialGrading::InterfaceClustered { strength: 1.0 },
        ] {
            let (mesh, _) = disk_tri_mesh_graded(1.0, or, 5, 24, g, g);
            let bnd = disk_boundary_nodes(&mesh, or);
            let n_bnd = bnd.iter().filter(|&&b| b).count();
            assert_eq!(
                n_bnd, 24,
                "boundary node count must be n_angular under {g:?}"
            );
            // p=2 interior-DOF mask still builds (same edge topology).
            let mask = disk_pec_interior_dofs2(&mesh, or);
            assert_eq!(mask.len(), 2 * mesh.edges().len() + 2 * mesh.n_tris());
        }
    }

    #[test]
    fn geometric_grading_step_ratio_matches_configured_factor() {
        // Adjacent radial steps inside a region must scale by `ratio`.
        let ratio = 1.5;
        // Single region (set core tiny so the cladding band dominates the
        // ring count we measure); inspect the cladding band's steps.
        let (mesh, _) = disk_tri_mesh_graded(
            1.0,
            5.0,
            8,
            48,
            RadialGrading::Uniform,
            RadialGrading::Geometric { ratio },
        );
        let rs = ring_radii(&mesh);
        // Cladding rings are the radii strictly greater than 1.0 (core edge).
        let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
        let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
            .chain(clad.windows(2).map(|w| w[1] - w[0]))
            .collect();
        for w in steps.windows(2) {
            let got = w[1] / w[0];
            assert!(
                (got - ratio).abs() < 1e-9,
                "geometric step ratio {got} ≠ configured {ratio}"
            );
        }
    }

    #[test]
    fn linear_grading_last_step_is_ratio_times_first() {
        let ratio = 3.0;
        let (mesh, _) = disk_tri_mesh_graded(
            1.0,
            5.0,
            8,
            48,
            RadialGrading::Uniform,
            RadialGrading::Linear { ratio },
        );
        let rs = ring_radii(&mesh);
        let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
        let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
            .chain(clad.windows(2).map(|w| w[1] - w[0]))
            .collect();
        let got = steps[steps.len() - 1] / steps[0];
        assert!(
            (got - ratio).abs() < 1e-9,
            "linear last/first step ratio {got} ≠ configured {ratio}"
        );
        // And the steps grow monotonically (ratio > 1).
        for w in steps.windows(2) {
            assert!(w[1] > w[0], "linear steps must increase for ratio > 1");
        }
    }

    #[test]
    fn interface_clustered_min_step_is_adjacent_to_interface() {
        // Cladding clustered toward its inner edge (r = a): the smallest
        // cladding step must be the first one (adjacent to r = a).
        let (mesh, _) = disk_tri_mesh_graded(
            1.0,
            5.0,
            8,
            48,
            RadialGrading::Uniform,
            RadialGrading::InterfaceClustered { strength: 3.0 },
        );
        let rs = ring_radii(&mesh);
        let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
        let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
            .chain(clad.windows(2).map(|w| w[1] - w[0]))
            .collect();
        let min_idx = steps
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(
            min_idx, 0,
            "interface-clustered min step must be adjacent to r=a; steps={steps:?}"
        );

        // Core clustered toward its outer edge (r = a): the smallest core
        // step must be the *last* one (adjacent to r = a).
        let (mesh2, _) = disk_tri_mesh_graded(
            1.0,
            5.0,
            8,
            48,
            RadialGrading::InterfaceClustered { strength: 3.0 },
            RadialGrading::Uniform,
        );
        let rs2 = ring_radii(&mesh2);
        let core: Vec<f64> = rs2.into_iter().filter(|&r| r <= 1.0 + 1e-9).collect();
        // core includes 0.0 (center) then the core rings up to 1.0.
        let csteps: Vec<f64> = core.windows(2).map(|w| w[1] - w[0]).collect();
        let cmin_idx = csteps
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(
            cmin_idx,
            csteps.len() - 1,
            "core interface-clustered min step must be the outermost (at r=a); steps={csteps:?}"
        );
    }

    #[test]
    fn pml_graded_supports_per_band_grading_and_conforms() {
        // The downstream graded-fiber experiment needs the PML mesher with
        // independent per-band grading; all three band boundaries stay fixed.
        let (mesh, tags) = disk_tri_mesh_pml_graded(
            1.0,
            2.0,
            3.0,
            6,
            36,
            RadialGrading::InterfaceClustered { strength: 2.0 },
            RadialGrading::Geometric { ratio: 1.3 },
            RadialGrading::Linear { ratio: 0.5 },
        );
        assert_eq!(tags.len(), mesh.n_tris());
        for t in &mesh.tris {
            assert!(signed_area(&mesh, t) > 0.0, "non-CCW PML graded triangle");
        }
        let rs = ring_radii(&mesh);
        for &b in &[1.0, 2.0, 3.0] {
            assert!(
                rs.iter().any(|&r| (r - b).abs() < 1e-9),
                "band boundary {b} not conformed under graded PML"
            );
        }
        // Tags present for all three regions.
        assert!(tags.contains(&REGION_CORE));
        assert!(tags.contains(&REGION_CLADDING));
        assert!(tags.contains(&REGION_PML));
    }

    #[test]
    fn worst_aspect_ratio_matches_uniform_disk_quality() {
        // Sanity: on the documented balanced-knob uniform mesh the exposed
        // worst aspect ratio agrees with the test helper / stays < 7.
        let (mesh, _) = disk_tri_mesh(1.0, 3.0, 4, 25);
        let exposed = worst_aspect_ratio(&mesh);
        let mut by_helper = 0.0_f64;
        for t in &mesh.tris {
            by_helper = by_helper.max(aspect_ratio(&mesh, t));
        }
        assert!((exposed - by_helper).abs() < 1e-9);
        assert!(exposed < 7.0, "uniform worst aspect {exposed} exceeded 7");
    }

    #[test]
    fn aspect_ratio_guard_accepts_mild_and_rejects_sliver_grading() {
        // Mild grading passes the checked constructor.
        let ok = disk_tri_mesh_graded_checked(
            1.0,
            3.0,
            5,
            31,
            RadialGrading::Geometric { ratio: 1.2 },
            RadialGrading::Geometric { ratio: 1.2 },
            ASPECT_RATIO_SLIVER_BOUND,
        );
        assert!(ok.is_ok(), "mild grading must pass the aspect guard");

        // Deliberately pathological grading (extreme geometric ratio with few
        // angular sectors) manufactures slivers and must be rejected.
        let sliver = disk_tri_mesh_graded_checked(
            1.0,
            3.0,
            12,
            6,
            RadialGrading::Geometric { ratio: 3.0 },
            RadialGrading::Geometric { ratio: 3.0 },
            ASPECT_RATIO_SLIVER_BOUND,
        );
        assert!(
            sliver.is_err(),
            "strong grading must be rejected by the aspect guard; worst aspect was {}",
            worst_aspect_ratio(
                &disk_tri_mesh_graded(
                    1.0,
                    3.0,
                    12,
                    6,
                    RadialGrading::Geometric { ratio: 3.0 },
                    RadialGrading::Geometric { ratio: 3.0 },
                )
                .0
            )
        );

        // PML checked constructor likewise guards.
        let pml_ok = disk_tri_mesh_pml_graded_checked(
            1.0,
            2.0,
            3.0,
            5,
            31,
            RadialGrading::Uniform,
            RadialGrading::Geometric { ratio: 1.2 },
            RadialGrading::Uniform,
            ASPECT_RATIO_SLIVER_BOUND,
        );
        assert!(pml_ok.is_ok());
    }
}
