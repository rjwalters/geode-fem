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
//! (the 2-D analogue of the 3-D tet formulas in `crate::elements::nedelec`).
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

use faer::Mat;
use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::complex::{CheckedComplexEigenpairs, SparseComplexShiftInvertLanczos};
use crate::eigen::dense::{EigenError, EigenPair};
use crate::eigen::lanczos::{CheckedEigenpairs, ConvergenceCheck, SparseShiftInvertLanczos};

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
/// [`crate::driven::ports::PortMode::beta`] under the `exp(+jωt)` time
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

/// Outgoing-wave complex `β` of a TE mode in a **homogeneously filled**
/// guide (issue #777), natural units (`c = 1`, `k₀ = ω`):
///
/// ```text
/// β² = ω²·ε_t·μ_t − (μ_t/μ_n)·k_c²
/// ```
///
/// with `ε_t` / `μ_t` the (isotropic) transverse permittivity /
/// permeability and `μ_n` the permeability along the guide axis; the
/// cutoff wavenumber `k_c` is the geometric (vacuum) one of the
/// cross-section. `ε_n` does not enter (a TE mode has `E_n = 0`).
///
/// * **Real `ε_t`** (`Im ε_t = 0`): the [`beta_outgoing`] branch —
///   `+√β²` above cutoff, `−j√(−β²)` below. With `ε_t = μ_t = μ_n = 1`
///   the arithmetic is exactly [`beta_outgoing`]`(ω, 1, k_c)`.
/// * **Complex `ε_t`** (lossy / dispersive fill): the principal root of
///   the complex `β²`, negated if needed so that `Im β ≤ 0` — the
///   decaying, outgoing branch of `exp(−jβz)` under `exp(+jωt)`. For a
///   lossy propagating mode this gives `Re β > 0`, `Im β < 0`; a lossy
///   below-cutoff mode keeps a small `Re β ≠ 0`.
pub fn beta_outgoing_filled(omega: f64, eps_t: c64, mu_t: f64, mu_n: f64, k_c: f64) -> c64 {
    let k_sq = omega * omega;
    let cut = (mu_t / mu_n) * (k_c * k_c);
    if eps_t.im == 0.0 {
        let arg = k_sq * eps_t.re * mu_t - cut;
        return if arg >= 0.0 {
            c64::new(arg.sqrt(), 0.0)
        } else {
            c64::new(0.0, -(-arg).sqrt())
        };
    }
    let beta = c64::new(k_sq * eps_t.re * mu_t - cut, k_sq * eps_t.im * mu_t).sqrt();
    if beta.im > 0.0 { -beta } else { beta }
}

/// TE10 **conductor attenuation** `α_c` [Np per length unit] of an
/// `a × b` rectangular guide with finitely conducting walls (issue #776),
/// natural units (`η₀ = 1`, `c = 1`, `ω = k₀`, lengths in mesh units):
///
/// ```text
/// α_c = R_s · (2bπ² + a³k²) / (a³ b β k η),
/// k = k₀√ε_r,   η = 1/√ε_r,   β = √(k² − (π/a)²),
/// R_s = √(k₀ / (2σ)),
/// ```
///
/// Pozar, *Microwave Engineering*, 4th ed. (Wiley 2012), §3.3,
/// Eq. (3.96), for a guide homogeneously filled with a lossless
/// dielectric `ε_r` (non-magnetic) and walls of natural-unit conductivity
/// `sigma_nat` (`σ_nat = σ_SI·η₀·L_unit`). `R_s` is exactly
/// `Re Z_s` of
/// [`crate::driven::solve::SurfaceImpedanceModel::GoodConductor`], so the
/// oracle uses the same surface resistance the solver applies; a rough
/// wall scales it by the roughness factor `K` (first order).
///
/// Returns `NaN` at or below the TE10 cutoff (`k ≤ π/a`).
pub fn te10_conductor_attenuation(a: f64, b: f64, k0: f64, eps_r: f64, sigma_nat: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let k = k0 * eps_r.sqrt();
    let eta = 1.0 / eps_r.sqrt();
    let beta_sq = k * k - (pi / a) * (pi / a);
    if beta_sq <= 0.0 {
        return f64::NAN;
    }
    let beta = beta_sq.sqrt();
    let r_s = (k0 / (2.0 * sigma_nat)).sqrt();
    r_s * (2.0 * b * pi * pi + a * a * a * k * k) / (a * a * a * b * beta * k * eta)
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
/// This is the 2-D analogue of [`crate::mesh::cube_tet_mesh`] — a programmatic
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

/// Multi-band concentric-ring disk mesh: generalizes [`disk_tri_mesh`] /
/// [`disk_tri_mesh_pml`] from two/three fixed material regions to an
/// **arbitrary number of concentric annular bands** for a machine
/// cross-section (Epic #448 Phase 2: back-iron / magnet / air-gap /
/// stator-iron, `μ_r ≫ 1` iron abutting an `μ_r = 1` air gap).
///
/// # Geometry
///
/// `radii = [r₀, r₁, …, r_B]` with `r₀ == 0` (the center) and strictly
/// increasing gives `B` bands, where band `k` occupies the annulus
/// `[r_k, r_{k+1})` (band `0` is the central disk `[0, r₁)`). Every
/// band boundary `r_k` is a fixed ring radius, so the mesh **conforms to
/// every material interface** (no triangle straddles a band boundary) and
/// the centroid-radius band test is unambiguous — exactly the conformity
/// guarantee [`disk_tri_mesh`] gives for the single core/cladding split,
/// extended to `B` bands.
///
/// `n_radial_per_band[k]` is the number of radial subdivisions inside band
/// `k` (each `≥ 1`); a thin band (e.g. a ~1 %-radius air gap) can be given
/// its own radial count independent of the thick bands. `gradings[k]`
/// distributes those rings within band `k` per [`RadialGrading`]. The
/// central band (`k = 0`) grades toward its outer edge; every outer band
/// grades toward its **inner** edge, matching the two/three-region meshers.
///
/// # Returns
///
/// `(mesh, band_tags)` where `band_tags[t] == k` (the band index, `0..B`)
/// for the band containing triangle `t`'s centroid. Feed `band_tags` into
/// [`crate::assembly::magnetostatic::build_nu_r`] with a per-band `μ_r`
/// table to get the per-triangle reluctivity `ν = 1/μ_r` the scalar
/// magnetostatic assembler consumes.
///
/// # Panics
///
/// Panics unless `radii.len() ≥ 3` (i.e. `≥ 2` bands; the machine use case
/// wants `≥ 4`), `radii[0] == 0`, `radii` is strictly increasing and finite,
/// `n_radial_per_band.len() == radii.len() − 1`, every entry is `≥ 1`,
/// `gradings.len() == radii.len() − 1`, and `n_angular ≥ 3`.
pub fn disk_tri_mesh_bands(
    radii: &[f64],
    n_angular: usize,
    n_radial_per_band: &[usize],
    gradings: &[RadialGrading],
) -> (TriMesh, Vec<i32>) {
    let n_bands = radii.len().saturating_sub(1);
    assert!(
        radii.len() >= 3,
        "disk_tri_mesh_bands requires ≥ 3 radii (≥ 2 bands); got {}",
        radii.len()
    );
    assert!(
        radii[0] == 0.0,
        "disk_tri_mesh_bands requires radii[0] == 0 (center); got {}",
        radii[0]
    );
    for w in radii.windows(2) {
        assert!(
            w[0].is_finite() && w[1].is_finite() && w[0] < w[1],
            "disk_tri_mesh_bands requires strictly increasing finite radii; got {w:?}"
        );
    }
    assert_eq!(
        n_radial_per_band.len(),
        n_bands,
        "disk_tri_mesh_bands requires n_radial_per_band.len() ({}) == n_bands ({n_bands})",
        n_radial_per_band.len()
    );
    assert!(
        n_radial_per_band.iter().all(|&n| n >= 1),
        "disk_tri_mesh_bands requires every n_radial_per_band entry ≥ 1"
    );
    assert_eq!(
        gradings.len(),
        n_bands,
        "disk_tri_mesh_bands requires gradings.len() ({}) == n_bands ({n_bands})",
        gradings.len()
    );
    assert!(n_angular >= 3, "disk_tri_mesh_bands requires n_angular ≥ 3");

    // Assemble the ring radii band-by-band. The central band uses the core
    // push (grades toward its outer edge); every outer band grades toward its
    // inner edge, exactly like the two/three-region meshers.
    let total_rings: usize = n_radial_per_band.iter().sum();
    let mut ring_r = Vec::with_capacity(total_rings + 1);
    ring_r.push(0.0);
    push_core_band(&mut ring_r, radii[1], n_radial_per_band[0], gradings[0]);
    for k in 1..n_bands {
        push_outer_band(
            &mut ring_r,
            radii[k],
            radii[k + 1],
            n_radial_per_band[k],
            gradings[k],
            InterfaceEdge::Inner,
        );
    }
    debug_assert_eq!(ring_r.len(), total_rings + 1);

    // Tag each triangle by which [r_k, r_{k+1}) band its centroid falls in.
    // The upper band caps at radii[n_bands] (== the outer radius); a centroid
    // exactly on the outer radius (it never is — centroids sit strictly
    // inside) would map to the last band by the `< r_{k+1}` fallthrough.
    let radii_owned = radii.to_vec();
    build_disk_mesh(&ring_r, n_angular, &move |r| {
        for k in 0..n_bands {
            if r < radii_owned[k + 1] {
                return k as i32;
            }
        }
        (n_bands - 1) as i32
    })
}

/// [`disk_tri_mesh_bands`] with a **mesh-quality guard**: if the worst
/// triangle aspect ratio exceeds `aspect_bound` (a sane default is
/// [`ASPECT_RATIO_SLIVER_BOUND`]), the configuration is rejected with an
/// `Err` describing the offending aspect ratio, rather than silently
/// returning a sliver-laden mesh. A thin (~1 %-radius) air-gap band is the
/// prime sliver risk, so this checked variant is the one the machine
/// cross-section meshing should use.
pub fn disk_tri_mesh_bands_checked(
    radii: &[f64],
    n_angular: usize,
    n_radial_per_band: &[usize],
    gradings: &[RadialGrading],
    aspect_bound: f64,
) -> Result<(TriMesh, Vec<i32>), String> {
    let (mesh, tags) = disk_tri_mesh_bands(radii, n_angular, n_radial_per_band, gradings);
    let worst = worst_aspect_ratio(&mesh);
    if worst > aspect_bound {
        return Err(format!(
            "disk_tri_mesh_bands_checked: worst aspect ratio {worst:.3} exceeds bound \
             {aspect_bound:.3} — thin band under-resolved or grading too strong \
             (radii={radii:?}, n_radial_per_band={n_radial_per_band:?})"
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
/// tag vector into `assemble_2d_nedelec2_pml_sparse_interior` to flag the
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

/// Build the **P2 Dirichlet mask** for a [`disk_tri_mesh`] of outer radius
/// `outer_radius`: the length-`n_nodes + n_edges` boolean mask over the
/// quadratic-Lagrange DOFs (vertex DOFs first, then edge-midpoint DOFs in
/// [`TriMesh::edges`] order) that pins every DOF on the outer boundary.
///
/// A vertex DOF is pinned when its node lies on the outer circle (exactly
/// [`disk_boundary_nodes`]); an edge-midpoint DOF is pinned when **both**
/// endpoints of the edge lie on the outer circle — i.e. the edge runs along
/// the boundary, so its midpoint is on the boundary too. Getting the
/// boundary-edge midpoints right is essential: leaving a boundary-edge
/// midpoint free would under-constrain the P2 system (the outer ring would
/// no longer be a closed Dirichlet contour). This is the P2 analogue of
/// [`disk_pec_interior_edges`]'s both-endpoints-on-boundary edge test.
pub fn disk_p2_boundary_dofs(mesh: &TriMesh, outer_radius: f64) -> Vec<bool> {
    let on_boundary = disk_boundary_nodes(mesh, outer_radius);
    let edges = mesh.edges();
    let mut mask = Vec::with_capacity(mesh.n_nodes() + edges.len());
    // Vertex DOFs: pinned iff the node is on the outer circle.
    mask.extend_from_slice(&on_boundary);
    // Edge-midpoint DOFs: pinned iff both endpoints are on the outer circle.
    for e in &edges {
        mask.push(on_boundary[e[0] as usize] && on_boundary[e[1] as usize]);
    }
    mask
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
/// Shared affine-triangle geometry: barycentric gradients, their Gram
/// matrix, the signed area, and the absolute area.
///
/// Every 2-D affine-triangle element kernel in this module (the
/// Whitney/Nédélec curl-curl of [`tri_nedelec_local`] and the scalar-P1
/// Poisson stiffness of [`tri_p1_local`]) needs the *same* barycentric
/// gradients `∇λ_p`, their Gram matrix `G_pq = ∇λ_p·∇λ_q`, and the element
/// area. Factoring the arithmetic here is a single source of truth so the
/// nodal-Lagrange and edge-Whitney paths cannot drift apart (the gradient
/// formula is written once, not copy-pasted).
///
/// `coords` are the three vertex coordinates `[v0, v1, v2]`, each `[x, y]`.
/// Returns `(grad, gram, signed_area, abs_area)` where
/// - `grad[p]  = ∇λ_p = ((y_{p+1}−y_{p+2}), (x_{p+2}−x_{p+1})) / det` (cyclic),
/// - `gram[p][q] = ∇λ_p·∇λ_q`,
/// - `signed_area = det/2` (positive for CCW vertex order),
/// - `abs_area   = |det|/2`.
pub(crate) fn tri_bary_grads(coords: &[[f64; 2]; 3]) -> ([[f64; 2]; 3], [[f64; 3]; 3], f64, f64) {
    // Edge vectors from v0.
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];

    // Signed double area (det of [e1 | e2]).
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let signed_area = 0.5 * det;
    let abs_area = 0.5 * det.abs();

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

    (grad, gram, signed_area, abs_area)
}

/// Closed-form local scalar-P1 (nodal Lagrange) stiffness and mass
/// matrices for an affine triangle — the element kernel of the 2-D
/// scalar-Poisson operator `−∇·(ν∇u) = f`.
///
/// `coords` are the three vertex coordinates `[v0, v1, v2]`, each `[x, y]`.
/// Returns `(k_local, m_local, signed_area)` where rows/columns index the
/// three **nodes** `(v0, v1, v2)` (nodal DOFs — unlike the edge-indexed
/// [`tri_nedelec_local`]):
///
/// ```text
///   K_pq = area · (∇λ_p·∇λ_q)          (Dirichlet-energy stiffness)
///   M_pq = (area / 12) · (1 + δ_pq)     (2-D consistent-mass constant)
/// ```
///
/// The barycentric gradients, their Gram matrix, and the area are computed
/// by the shared `tri_bary_grads` helper — the *same* arithmetic
/// [`tri_nedelec_local`] uses — so the P1 and Nédélec element geometry
/// cannot diverge.
///
/// The unweighted `K` matches the material-independent stiffness; a
/// per-element reluctivity `ν = 1/μ_r` is applied at assembly time (`ν`
/// weights the *stiffness* here, the dual of the `ε`-weights-mass pattern
/// of the Nédélec modal solver).
pub fn tri_p1_local(coords: &[[f64; 2]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3], f64) {
    let (_grad, gram, signed_area, area_abs) = tri_bary_grads(coords);

    let mut k_local = [[0.0_f64; 3]; 3];
    let mut m_local = [[0.0_f64; 3]; 3];
    for p in 0..3 {
        for q in 0..3 {
            // Stiffness: ∫ ∇λ_p·∇λ_q dA = area · G_pq (constant integrand).
            k_local[p][q] = area_abs * gram[p][q];
            // Consistent mass: ∫ λ_p λ_q dA = (area/12)(1 + δ_pq).
            let delta = if p == q { 1.0 } else { 0.0 };
            m_local[p][q] = (area_abs / 12.0) * (1.0 + delta);
        }
    }

    (k_local, m_local, signed_area)
}

pub fn tri_nedelec_local(coords: &[[f64; 2]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3], f64) {
    let (_grad, gram, area, area_abs) = tri_bary_grads(coords);

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

/// Closed-form local **quadratic (P2) scalar Lagrange** stiffness and mass
/// matrices for an affine triangle — the second-order element kernel of the
/// 2-D scalar-Poisson operator `−∇·(ν∇u) = f`.
///
/// Second-order sibling of [`tri_p1_local`]. The six DOFs are the three
/// vertices plus the three edge midpoints, in the order
/// `[v0, v1, v2, e0, e1, e2]` where the edges follow [`TRI_LOCAL_EDGES`]
/// (`e0 = (0,1)`, `e1 = (0,2)`, `e2 = (1,2)`). This matches the global
/// DOF numbering the P2 magnetostatic assembler uses: vertex DOFs keep the
/// node index; edge-midpoint DOFs are offset by `n_nodes` via
/// [`TriMesh::edges`]/[`TriMesh::tri_edges`] (the edge *sign* is irrelevant
/// for unsigned scalar Lagrange DOFs, unlike the signed Nédélec assembler).
///
/// The quadratic basis in barycentric coordinates is
///
/// ```text
///   vertex p:   φ_p   = λ_p (2 λ_p − 1) ,   ∇φ_p  = (4 λ_p − 1) ∇λ_p
///   edge (a,b): φ_ab  = 4 λ_a λ_b ,         ∇φ_ab = 4 (λ_a ∇λ_b + λ_b ∇λ_a)
/// ```
///
/// so the basis gradients are **linear** in the barycentrics; the stiffness
/// integrand `∇φ_i·∇φ_j` is degree-2 and the mass integrand `φ_i φ_j` is
/// degree-4. Both are integrated exactly by the shared degree-4 rule
/// [`TRI_QUAD_DEG4`] (the same rule [`tri_nedelec2_local`] uses), and the
/// barycentric gradients / signed area come from the shared
/// `tri_bary_grads` helper so the P1, P2, and Nédélec element geometry
/// cannot drift apart.
///
/// Returns `(k_local, m_local, signed_area)` where `k_local` (6×6) is the
/// Dirichlet-energy stiffness `∫ ∇φ_p·∇φ_q dA`, `m_local` (6×6) is the
/// consistent mass `∫ φ_p φ_q dA`, and the signed area is positive for CCW
/// vertex order. As with [`tri_p1_local`], the per-element reluctivity
/// `ν = 1/μ_r` weights the stiffness at assembly time.
pub fn tri_p2_local(coords: &[[f64; 2]; 3]) -> ([[f64; 6]; 6], [[f64; 6]; 6], f64) {
    let (grad, _gram, signed_area, area_abs) = tri_bary_grads(coords);

    // Evaluate the six P2 basis values and gradients at a barycentric point
    // `lam = (λ0, λ1, λ2)`. Ordering: [v0, v1, v2, e0, e1, e2].
    let eval = |lam: [f64; 3]| -> ([f64; 6], [[f64; 2]; 6]) {
        let mut val = [0.0_f64; 6];
        let mut gr = [[0.0_f64; 2]; 6];
        // Vertex DOFs.
        for p in 0..3 {
            let lp = lam[p];
            val[p] = lp * (2.0 * lp - 1.0);
            let c = 4.0 * lp - 1.0;
            gr[p] = [c * grad[p][0], c * grad[p][1]];
        }
        // Edge-midpoint DOFs, in TRI_LOCAL_EDGES order.
        for (e, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let (la, lb) = (lam[a], lam[b]);
            val[3 + e] = 4.0 * la * lb;
            gr[3 + e] = [
                4.0 * (la * grad[b][0] + lb * grad[a][0]),
                4.0 * (la * grad[b][1] + lb * grad[a][1]),
            ];
        }
        (val, gr)
    };

    let mut k_local = [[0.0_f64; 6]; 6];
    let mut m_local = [[0.0_f64; 6]; 6];
    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area_abs; // physical-area quadrature weight
        let (val, gr) = eval(lam);
        for i in 0..6 {
            for j in 0..6 {
                k_local[i][j] += w * (gr[i][0] * gr[j][0] + gr[i][1] * gr[j][1]);
                m_local[i][j] += w * val[i] * val[j];
            }
        }
    }

    (k_local, m_local, signed_area)
}

/// Assemble dense global Whitney/Nédélec stiffness `K` (curl-curl) and
/// mass `M` for a 2-D triangle mesh.
///
/// Returns `(K, M)` of size `[n_edges, n_edges]`. Triangle-local 3×3
/// blocks are scattered with the per-DOF sign that records the local-vs-
/// global edge orientation (`crate::analytic::waveguide::TriMesh::tri_edges`).
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
/// [`crate::assembly::nedelec::assemble_global_nedelec_with_epsilon`]
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
/// taken from `TRI_NEDELEC2_DOF_FLIPS` via `tri_nedelec2_dofs`: the
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
/// point, the 2D reduction of [`crate::driven::scattering::upml_matched_tensors`]
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
/// centroid, exactly as the 3D [`crate::driven::scattering::build_matched_upml_materials`]
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
/// [`crate::assembly::nedelec::build_epsilon_r`]: a fixture labels each
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
pub(crate) fn estimate_modal_shift(
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
    let diag_trace = |a: SparseColMatRef<'_, usize, f64>| {
        let (cp, ri, v) = (a.col_ptr(), a.row_idx(), a.val());
        (0..a.ncols())
            .flat_map(|j| (cp[j]..cp[j + 1]).filter(move |&k| ri[k] == j))
            .map(|k| v[k].abs())
            .sum::<f64>()
    };
    // Spectral scale `τ = tr K / tr M`, a mean diagonal Rayleigh quotient:
    // an upper-spectrum eigenvalue scale `∝ 1/h²` that, like every
    // eigenvalue of the pencil, scales as `L⁻²` with the mesh length unit
    // (issue #828). The physical cutoffs sit at `≈ (h/a)²·τ` or above.
    let (k_trace, m_trace) = (diag_trace(k_sparse), diag_trace(m_sparse));
    let tau = if m_trace > 0.0 && k_trace > 0.0 {
        k_trace / m_trace
    } else {
        1.0
    };
    // Probe shift: 1e-10 · τ. Sits ~10 orders of magnitude below the
    // spectral scale, far below any physical mode on any mesh this targets,
    // in any length unit. Large enough to make A = K − σM non-singular
    // (the gradient nullspace sits at round-off, `≈ ε·τ`) even when K has a
    // 100+-dim gradient nullspace. (It was `1e-10 · max(tr M / n, 1)`: the
    // mean mass diagonal is not an eigenvalue scale, and the floor made it
    // the absolute `1e-10` on most meshes.)
    let probe_sigma = 1e-10_f64 * tau;
    let probe = SparseShiftInvertLanczos {
        sigma: probe_sigma,
        max_iters: probe_budget,
        tol: 1e-6,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
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
    // The `1e-12 · τ` floor (scale-covariant, issue #828; it was the absolute
    // `1e-12`) only matters if every probed λ is noise.
    let max_lambda = pairs.iter().map(|p| p.lambda.abs()).fold(0.0_f64, f64::max);
    let cluster_threshold = (1e-6_f64 * max_lambda).max(1e-12 * tau);
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
/// # Robustness: a small reference basis, and a loud guard
///
/// A single fixed `F` can be (near-)orthogonal to some modes — e.g. a
/// uniform x-directed field has zero net projection onto a mode that is
/// x-antisymmetric (TE₂₀-like). To stay well-defined for every mode we
/// try an ordered list of reference fields and use the **first** whose
/// projection magnitude clears a relative floor. The list is fixed and
/// mesh-independent, so two meshes resolving the same physical mode
/// select the same reference and therefore the same sign.
///
/// The reference basis only spans the lowest rectangular-guide modes
/// (up to ~TE₀₂ plus uniform catch-alls). For a mode it does **not**
/// span — a higher-order mode, or any mode on a non-rectangular cross-
/// section that is orthogonal to all listed fields — *every* projection
/// is negligible. Previously the helper fell back to
/// [`pin_eigenvector_sign`] in that case, which is deterministic per call
/// but cross-mesh-**unstable** (its argmax pivot DOF can jump between
/// meshes, flipping the sign — the exact bug #300 fixed). That silent
/// fall-through reintroduced the hazard for any mode outside the basis.
///
/// This helper therefore makes the fall-through **loud** (issue #349): if
/// no reference clears the floor it returns
/// [`EigenError::UngaugableMode`] instead of silently using the unstable
/// argmax pin. A caller hitting this error knows the reference basis must
/// be extended (or a general functional adopted) before that mode can be
/// gauged cross-mesh-stably — there is no silent-but-wrong path. An
/// all-zero eigenvector (e.g. a fully PEC-eliminated profile) has no sign
/// to pin and is returned as a benign `Ok(())` no-op.
///
/// # Invariants
///
/// The rotation is a unit-modulus scalar (here `±1`), so it is
/// **norm-preserving**: `eᵀ M e` and the set-wise `e_iᵀ M e_j` Gram
/// entries are unchanged. M-orthonormality of the returned set is
/// therefore preserved exactly (see the unit test
/// `reference_gauge_preserves_orthonormality`).
///
/// # Errors
///
/// Returns [`EigenError::UngaugableMode`] when the fixed reference basis
/// does not span the mode (every projection below the relative floor),
/// carrying the largest relative projection observed for diagnosis.
fn gauge_fix_eigenvector(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    e_edges: &mut [f64],
    mode: usize,
) -> Result<(), EigenError> {
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
    let e_scale = e_edges.iter().fold(0.0_f64, |acc, &x| acc + x * x).sqrt();

    // An all-zero eigenvector (e.g. a fully PEC-eliminated profile) has no
    // sign to pin: there is no cross-mesh-unstable argmax to guard against,
    // so this is a benign no-op rather than an ungaugable-mode error.
    if e_scale <= f64::EPSILON {
        return Ok(());
    }

    // Track the largest *relative* projection seen across all references,
    // so a loud failure can report how far below the floor the best
    // reference fell (purely diagnostic).
    let mut best_rel_proj = 0.0_f64;

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
        // Relative projection: |p| as a fraction of the Cauchy–Schwarz
        // ceiling |e|·|r|. A field orthogonal to this mode produces ≈ 0.
        let ceiling = e_scale * ref_scale;
        if ceiling > 0.0 {
            best_rel_proj = best_rel_proj.max(proj.abs() / ceiling);
        }
        // Relative floor: require the projection to be a non-trivial
        // fraction of the ceiling. A field that is orthogonal to this mode
        // produces proj ≈ 0 and is skipped.
        let floor = 1e-6 * ceiling;
        if proj.abs() > floor {
            if proj < 0.0 {
                for x in e_edges.iter_mut() {
                    *x = -*x;
                }
            }
            return Ok(());
        }
    }

    // Loud guard (issue #349): no reference overlapped this mode. Rather
    // than silently falling through to `pin_eigenvector_sign` — whose
    // argmax pivot is cross-mesh-unstable and would reintroduce the #300
    // sign-flip — refuse to gauge it. The fixed reference basis only spans
    // the lowest rectangular-guide modes; a caller hitting this must
    // extend the basis (or adopt a general functional) before this mode
    // can be pinned cross-mesh-stably.
    Err(EigenError::UngaugableMode {
        mode,
        best_rel_proj,
    })
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
/// gauge** (`gauge_fix_eigenvector`): the eigenvector is rotated so its
/// projection onto a fixed continuous reference profile is real-positive.
/// Concretely, a smooth reference vector field `F(x, y)` is sampled at
/// each edge midpoint to form the Whitney DOF it would produce, and the
/// eigenvector is negated iff its inner product with that reference is
/// negative.
///
/// This gives a gauge that is reproducible **across mesh refinements at
/// the level of the raw complex S-matrix entries**, not merely
/// deterministic per call. The earlier convention (issue #262,
/// `pin_eigenvector_sign`) pinned on the single largest-magnitude DOF;
/// because that pivot DOF is a property of the discretization, it could
/// jump to a different edge between meshes and flip the sign — so the C2
/// mode-matching test had to compare gauge-invariant *magnitudes* (PR
/// #261 documented the `nx = 10 → nx = 16` flip). The reference-integral
/// projection is a quadrature of the *continuous* functional `∫ e · F dS`,
/// so it converges with the mesh instead of jumping, pinning both sign
/// and (in the complex generalization) phase consistently regardless of
/// mesh. See `gauge_fix_eigenvector` for the convention's rationale.
///
/// The fixed reference basis only spans the lowest rectangular-guide
/// modes (up to ~TE₀₂ plus uniform catch-alls). For a mode it does not
/// span — a higher-order or non-rectangular case — the gauge no longer
/// falls through silently to the cross-mesh-unstable argmax pin; instead
/// this solver returns [`EigenError::UngaugableMode`] (issue #349), so
/// the limitation is loud rather than a silent sign-flip hazard. In
/// scope (the metallic rectangular guide, lowest few modes) every mode
/// is spanned and this error never fires.
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
/// `pin_eigenvector_sign` argmax pin; extending the reference-integral
/// phase gauge to them is the documented complex generalization in
/// `gauge_fix_eigenvector` but is out of scope for issue #300.
///
/// # Solver
///
/// Uses [`crate::eigen::lanczos::SparseShiftInvertLanczos`] (real-symmetric
/// sparse shift-and-invert Lanczos via faer's sparse LU). The 2-D
/// modal pencil is real-symmetric SPD after PEC reduction, and the
/// gradient null cluster sits at λ ≈ 0; a small positive shift (see
/// `modal_shift`) targets the lowest physical modes while keeping the
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
    /// between zero and that estimate (see `estimate_modal_shift`).
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
/// (see `estimate_modal_shift`) and the spurious-mode threshold is
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
///    `estimate_modal_shift`). Set `σ = 0.5 · λ_min_phys`.
/// 3. Set the spurious-mode threshold to `0.1 · σ` (one decade of
///    slack below the shift; the gradient cluster sits many decades
///    below `σ`).
/// 4. Run the production shift-invert Lanczos with the estimated `σ`
///    and filter out cluster modes by threshold. The pass is
///    residual-checked (issue #798): only Ritz pairs with relative residual
///    `‖K x − λ M x‖/(λ‖M x‖) ≤ 1e-8` are returned, and an unconverged pair
///    above the threshold extends the Lanczos run (bounded) until it
///    converges.
/// 5. If the filtered count is short, or a withheld unconverged pair sits
///    below the last returned mode, double the Lanczos budget and retry
///    (Approach B in issue #265). If the budget reaches the pencil
///    dimension first, return an error naming the shortfall. A short list
///    is never returned silently.
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
    solve_metallic_modes(mesh, edges, interior_edge_mask, n_modes, opts, true).map(|(m, _)| m)
}

/// [`solve_waveguide_modes`] **without** the reference-integral sign gauge
/// (issue #888): the lowest `n_modes` modes with the Lanczos signs, plus the
/// further converged modes of the same pass (`λ` ascending, possibly
/// empty, not checked for completeness). The first list is bit-identical
/// to [`solve_waveguide_modes`] up to the sign of each vector. The port-face
/// path applies its own canonical gauge
/// ([`crate::driven::ports::PortFaceProjection::solve_modes`]) and reads the
/// second list to complete a degenerate cluster cut by `n_modes`.
///
/// `sigma` is an explicit shift-invert shift (`None`: the probe estimator
/// [`estimate_modal_shift`], as [`solve_waveguide_modes`]). The probe
/// budget grows with the gradient null space, which makes it the dominant,
/// roughly cubic cost of the solve; a caller that already knows the
/// spectrum (the degenerate-cluster confirmation of issue #892, which knows
/// the face's lowest cutoff from the solve it confirms) passes
/// `σ = λ_first / 2` and skips it.
pub(crate) fn solve_waveguide_modes_ungauged(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    sigma: Option<f64>,
) -> Result<(Vec<WaveguideModeProfile>, Vec<WaveguideModeProfile>), EigenError> {
    let opts = WaveguideSolveOpts {
        sigma,
        spurious_threshold: None,
        sigma_relative_threshold: 0.1,
    };
    solve_metallic_modes(mesh, edges, interior_edge_mask, n_modes, &opts, false)
}

/// The metallic modal solve behind [`solve_waveguide_modes_with_opts`]
/// (`gauge = true`) and [`solve_waveguide_modes_ungauged`] (`gauge =
/// false`): the requested modes and the further converged modes of the
/// final pass.
fn solve_metallic_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    opts: &WaveguideSolveOpts,
    gauge: bool,
) -> Result<(Vec<WaveguideModeProfile>, Vec<WaveguideModeProfile>), EigenError> {
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
    //
    // Issue #798: the pass is residual-checked. Only converged pairs come
    // back (unconverged pairs above the threshold extend the Lanczos run),
    // and a withheld pair below the last returned mode, which could hide a
    // missing mode, triggers the same retry as an undercount.
    let mut n_request = (n_modes + 8).min(dim);
    let scatter = |pair: &EigenPair| {
        let mut e_edges = vec![0.0_f64; n_edges];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = pair.vector[interior_idx];
        }
        WaveguideModeProfile {
            k_c: pair.lambda.max(0.0).sqrt(),
            lambda: pair.lambda,
            e_edges,
        }
    };
    let modes = loop {
        let pass = metallic_checked_modes(
            k_sparse.as_ref(),
            m_sparse.as_ref(),
            sigma,
            threshold,
            n_request,
            n_modes,
        )?;
        let unresolved = pass.unresolved_below;
        let withheld = pass.withheld;

        let physical: Vec<WaveguideModeProfile> = pass
            .physical
            .into_iter()
            .enumerate()
            .map(|(mode_idx, pair)| {
                let mut profile = scatter(&pair);
                if !gauge {
                    return Ok(profile);
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
                //
                // If the fixed reference basis does not span this mode
                // (issue #349) the gauge returns `UngaugableMode` rather
                // than silently falling through to the cross-mesh-unstable
                // argmax pin; propagate it so the failure is loud.
                gauge_fix_eigenvector(mesh, edges, &mut profile.e_edges, mode_idx)?;
                Ok(profile)
            })
            .collect::<Result<Vec<_>, EigenError>>()?;

        if physical.len() == n_modes && !unresolved {
            break (physical, pass.beyond.iter().map(scatter).collect());
        }
        if n_request >= dim {
            let est_msg = est_first_phys
                .map(|f| format!(" (probe estimated λ_min_phys = {f:.3e})"))
                .unwrap_or_default();
            return Err(EigenError::FaerGevd(format!(
                "waveguide modal solve: only recovered {} of {} converged physical modes \
                 (filtered out spurious cluster at λ ≤ {threshold:.3e}, \
                 σ = {sigma:.3e}{est_msg}; {withheld} unconverged Ritz pair(s) withheld)",
                physical.len(),
                n_modes
            )));
        }
        n_request = (n_request * 2).min(dim);
    };
    Ok(modes)
}

/// Largest accepted relative true residual `‖K x − λ M x‖₂ / (λ ‖M x‖₂)` of
/// a metallic (cutoff) modal eigenpair (issue #798).
///
/// Measured on the `b/16` rectangular guide (`rect_tri_mesh(32, 16, 2, 1)`,
/// the PR #809 cross-check), with the historical `n_request + 8` budget:
/// converged physical pairs have residuals of `10⁻¹²…10⁻⁶`, and the tail
/// decays from `≈ 1` to `3×10⁻³`. That tail produced TE₃₁ off by
/// `1.06×10⁻⁸` and merged the near-degenerate pairs at `k_c² ≈ 39.31` and
/// `49.34` into single values. With this tolerance the checked solve returns
/// every pair at `≤ 4×10⁻¹⁰`, reproducing the `n_modes = 20` reference to
/// `10⁻¹³`. The same value as [`DIELECTRIC_RESIDUAL_TOL`].
const MODAL_RESIDUAL_TOL: f64 = 1e-8;

/// Cap on the metallic modal Lanczos extension, as a multiple of the
/// first-pass budget `n_request + 8` (issue #798). The `b/16` cross-check
/// converged in 68 of its 168 allowed steps.
const MODAL_LANCZOS_CAP_FACTOR: usize = 6;

/// Converged physical modes of one metallic modal Lanczos pass (issue #798).
pub(crate) struct MetallicModalPass {
    /// Converged pairs with `λ > threshold`, `λ` ascending, at most
    /// `n_modes`.
    pub(crate) physical: Vec<EigenPair>,
    /// The further converged pairs of the same pass above `physical`,
    /// `λ` ascending (issue #888: the canonical port-mode gauge reads them
    /// to complete a degenerate cluster cut by `n_modes`). Not checked for
    /// completeness: an unconverged pair may sit between them.
    pub(crate) beyond: Vec<EigenPair>,
    /// A withheld (unconverged) Ritz pair sits above the threshold but below
    /// the last returned mode, so a mode could be missing from the list.
    pub(crate) unresolved_below: bool,
    /// Unconverged Ritz pairs above the threshold that were withheld.
    pub(crate) withheld: usize,
}

/// One residual-checked shift-invert Lanczos pass of the metallic pencil
/// `K x = λ M x` near `σ` (issue #798). Only converged pairs are returned;
/// unconverged pairs above `threshold` extend the run, up to
/// [`MODAL_LANCZOS_CAP_FACTOR`] times the historical budget
/// `n_request + 8` (see
/// [`SparseShiftInvertLanczos::smallest_eigenpairs_checked`]).
pub(crate) fn metallic_checked_modes(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    threshold: f64,
    n_request: usize,
    n_modes: usize,
) -> Result<MetallicModalPass, EigenError> {
    let dim = k.nrows();
    let max_iters = (n_request + 8).min(dim).max(1);
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    let mut checked = solver.smallest_eigenpairs_checked(
        k,
        m,
        n_request,
        ConvergenceCheck {
            residual_tol: MODAL_RESIDUAL_TOL,
            max_iters_cap: (max_iters * MODAL_LANCZOS_CAP_FACTOR).min(dim),
            window: Some((threshold, f64::INFINITY)),
        },
    )?;
    checked.pairs.sort_by(|a, b| {
        a.lambda
            .partial_cmp(&b.lambda)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut physical: Vec<EigenPair> = checked
        .pairs
        .into_iter()
        .filter(|p| p.lambda > threshold)
        .collect();
    let beyond = physical.split_off(n_modes.min(physical.len()));
    let above: Vec<f64> = checked
        .rejected
        .iter()
        .map(|&(lambda, _)| lambda)
        .filter(|&lambda| lambda > threshold)
        .collect();
    let unresolved_below = physical
        .last()
        .is_some_and(|last| above.iter().any(|&lambda| lambda < last.lambda));
    Ok(MetallicModalPass {
        physical,
        beyond,
        unresolved_below,
        withheld: above.len(),
    })
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
/// largest-magnitude component is non-negative (`pin_eigenvector_sign`,
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
/// `estimate_modal_shift` for the analogous metallic shift-placement
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
/// eigenpairs below a floor centred in that gap (`3×10⁻²`) for high-contrast
/// cross-sections, scaled down with the index contrast for weakly-guiding
/// ones (issue #794); see `physical_curl_floor` for the scaling and for why
/// the floor depends only on the materials rather than on any adaptive
/// gap-widening (an out-of-window spike can drive widening above the genuine
/// band and return zero modes).
///
/// ## Unconverged Ritz pairs (issue #798)
///
/// The eigensolve returns only Ritz pairs whose true residual
/// `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` is at most `1e-8`, extending the
/// Lanczos run (bounded) until the requested window converges. Pairs still
/// unconverged at the cap are withheld, and the count is logged; a localized
/// withheld pair that would rank inside the returned set is a selection
/// hole (issue #850, see Errors). In-window
/// pairs that break the exact Rayleigh identity are rejected as a second
/// line (`rayleigh_consistent`).
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
/// Returns [`EigenError::SelectionHole`] if a localized, bound-like withheld
/// Ritz pair still sits above the lowest returned mode after one automatic
/// retry with a doubled Lanczos request (issue #850; see `selection_hole`).
/// When the solve finds **no** bound mode,
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
pub fn solve_dielectric_modes(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    // Request a generous batch so the physical band and the
    // gradient-nullspace band are both sampled and the gap can be detected.
    let n_request = (n_modes + 8).max(16);
    retry_on_selection_hole("solve_dielectric_modes", n_request, |n_request| {
        solve_dielectric_modes_attempt(mesh, eps_r, interior_edge_mask, k0, n_modes, n_request)
    })
}

/// One [`solve_dielectric_modes`] solve + classification at a given Lanczos
/// request, returning the bound modes and the outcome of the selection-level
/// hole check (issue #850).
fn solve_dielectric_modes_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricMode>, EigenError> {
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
    let raw = dielectric_raw_candidates_with_target(
        mesh,
        eps_r,
        interior_edge_mask,
        k0,
        n_request,
        index_ceiling,
    )?;
    let cands = &raw.cands;
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
    let curl_floor = physical_curl_floor(eps_max, eps_min);

    // ----- Classify -------------------------------------------------
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_edges);
    for (full_idx, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    let mut bound: Vec<DielectricMode> = Vec::new();
    let mut n_dropped = 0usize;
    let mut n_inconsistent = 0usize;
    for c in cands {
        debug_assert!(c.residual <= DIELECTRIC_RESIDUAL_TOL);
        let in_window = c.beta_sq > beta_sq_floor && c.beta_sq < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        // Second line behind the residual check (issues #791, #798): an
        // in-window pair that breaks the exact Rayleigh identity is not an
        // eigenpair of the pencil.
        if !rayleigh_consistent(c, k0, eps_max, eps_min) {
            n_dropped += 1;
            n_inconsistent += 1;
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
         dropped {n_dropped} radiation/spurious eigenpair(s), {n_inconsistent} of \
         them failing the Rayleigh identity; withheld {} unconverged Ritz pair(s) \
         (residual > {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps), {} of them \
         in the guided window (requested {n_modes})",
        raw.withheld, raw.lanczos_steps, raw.withheld_in_window
    );
    // Selection-level hole check (issue #850): a localized withheld pair
    // above the lowest returned mode would have ranked inside this set.
    // With no bound mode found the empty-set rule applies, which only
    // warns (issue #913). `n_modes = 0` asks for nothing, so nothing can
    // be missing.
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole_real(
            "solve_dielectric_modes",
            &raw,
            (beta_sq_floor, beta_sq_ceiling),
            bound.last().map(|m| m.beta_sq),
            &HoleRule::new(curl_floor, eps_max, eps_min, Some(k0)),
        )
    };
    Ok((bound, hole))
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
    /// Field-weighted permittivity `⟨ε⟩_x = (xᵀ M_ε x)/(xᵀ M₁ x)` of the
    /// eigenvector, a convex average of the per-triangle `ε_r`, so it lies
    /// in `[ε_min, ε_max]`. For a converged eigenpair it ties `β²` and
    /// [`Self::curl_ratio`] together through the Rayleigh identity
    /// `r = 1 − β²/(k₀²⟨ε⟩_x)` (issue #791).
    pub eps_weighted: f64,
    /// Relative true residual `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` of the pair
    /// (issue #798). Every candidate the solver returns has
    /// `residual ≤ DIELECTRIC_RESIDUAL_TOL`.
    pub residual: f64,
    /// Interior-DOF eigenvector (length = number of interior edges).
    pub vector: Vec<f64>,
}

/// The converged raw eigenpairs of one dielectric pencil solve, plus the
/// convergence record (issue #798). Produced by
/// [`dielectric_raw_candidates_with_target`] (p=1) and
/// `dielectric_raw_candidates_p2`.
pub(crate) struct RawDielectricSolve {
    /// Converged candidates (`residual ≤ DIELECTRIC_RESIDUAL_TOL`), sorted
    /// by decreasing `β²`.
    pub cands: Vec<RawDielectricCandidate>,
    /// Unconverged Ritz pairs of the requested set that were withheld (see
    /// [`CheckedEigenpairs::rejected`]).
    pub withheld: usize,
    /// How many of [`Self::withheld`] lie inside the guided `β²` window, the
    /// only ones that could have become modes. This is the shortfall that
    /// matters to the caller.
    pub withheld_in_window: usize,
    /// The **localized** withheld pairs (see
    /// [`CheckedEigenpairs::localized_withheld`]): eigenvalues still
    /// unconverged at the cap, each with the curl-energy ratio of its Ritz
    /// vector. The classifier tests them for a hole in its selection
    /// ([`selection_hole`], issues #850, #916).
    pub localized_withheld: Vec<WithheldCandidate>,
    /// Shift `σ` of the solve (sets each withheld pair's uncertainty
    /// `ρ · max(|λ|, |σ|)` in [`selection_hole`]).
    pub sigma: f64,
    /// Lanczos steps run (first pass plus any extension).
    pub lanczos_steps: usize,
}

/// Largest accepted relative true residual
/// `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` of a dielectric eigenpair (issue #798).
///
/// # Calibration
///
/// Measured on the PEC fiber (SMF-28 and ~3 %-step, mesh (5,48)), SiN strip
/// and Si slab fixtures, p=1 and p=2, with the historical
/// `max_iters = n_request + 8` budget: converged pairs have residuals of
/// `10⁻¹⁰…10⁻¹⁵`, and the unconverged tail of the window decays
/// geometrically from `≈ 0.5` to `≈ 10⁻⁷`, one to two decades per Ritz pair.
/// The Rayleigh identity violation of a pair is second order in its residual
/// (`δ ≈ ρ²`). The SMF-28 tail pair that [`rayleigh_consistent`] caught with a
/// 1.4× margin (`δ = 1.1×10⁻⁵`) has `ρ = 3.5×10⁻³`. Pairs with `ρ ≈ 10⁻²`
/// pass the Rayleigh check (`δ = 3×10⁻⁵` on the SiN strip, `8×10⁻⁵` on the
/// p=1 Si slab) while sitting in the guided window above the curl floor.
/// `10⁻⁸` rejects every tail pair above it, sits at least a decade above
/// every converged pair, and bounds the eigenvalue error of an accepted pair
/// at `O(ρ²)`.
const DIELECTRIC_RESIDUAL_TOL: f64 = 1e-8;

/// Cap on the dielectric Lanczos extension, as a multiple of the first-pass
/// Krylov dimension `n_request + 8` (issue #798). With `n_request = 16` the
/// first pass is 24 steps and the cap 144. Across the waveguide unit tests,
/// the fiber, SOI and audit benchmarks and the SMF-28 timing mesh, every
/// solve confirmed its in-window pairs within 24–88 steps, well inside the
/// cap. Past the cap the unconfirmed remainder is withheld and logged.
const DIELECTRIC_LANCZOS_CAP_FACTOR: usize = 6;

/// Shift-invert Lanczos solve of a dielectric pencil `A x = β² M₁ x` near
/// `σ`, returning only **converged** pairs (issue #798).
///
/// The first pass is the historical `max_iters = n_req + 8` solve. When one
/// of its pairs inside `guided_window` (the `β²` interval the classifier
/// keeps) misses [`DIELECTRIC_RESIDUAL_TOL`] but locates an eigenvalue, the
/// Krylov space is extended, up to [`DIELECTRIC_LANCZOS_CAP_FACTOR`] times
/// that budget, until a converged pair confirms it (see
/// [`SparseShiftInvertLanczos::smallest_eigenpairs_checked`]). Unconverged
/// pairs outside the window never extend the run. Everything not confirmed
/// is withheld and counted in [`CheckedEigenpairs::rejected`].
///
/// As for `pml_checked_eigenpairs`, the contract is the classifier's
/// selection from the converged set, not "the modes nearest `σ`", so no
/// solver-level hole check applies (PR #847). The hole rule runs at the
/// selection instead: the localized withheld pairs travel with the
/// candidates ([`RawDielectricSolve::localized_withheld`]) and each
/// classifier checks them against the bound modes it returns
/// ([`selection_hole`], issue #850).
fn dielectric_checked_eigenpairs(
    a: SparseColMatRef<'_, usize, f64>,
    m1: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    n_request: usize,
    dim: usize,
    guided_window: (f64, f64),
) -> Result<CheckedEigenpairs, EigenError> {
    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    solver.smallest_eigenpairs_checked(
        a,
        m1,
        n_req,
        ConvergenceCheck {
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            max_iters_cap: (max_iters * DIELECTRIC_LANCZOS_CAP_FACTOR).min(dim),
            window: Some(guided_window),
        },
    )
}

/// Build the [`RawDielectricSolve`] from a checked eigensolve: per-pair curl
/// ratio and field-weighted permittivity, sorted by decreasing `β²`.
fn raw_dielectric_solve(
    checked: CheckedEigenpairs,
    sigma: f64,
    guided_window: (f64, f64),
    curl_ratio: impl Fn(&[f64]) -> f64,
    eps_weighted: impl Fn(&[f64]) -> f64,
) -> RawDielectricSolve {
    let withheld = checked.rejected.len();
    // Issue #916: the curl ratio of each localized withheld Ritz vector, so
    // the hole check can skip curl-free gradient-nullspace pairs.
    let localized_withheld: Vec<WithheldCandidate> = checked
        .localized_withheld
        .iter()
        .map(|w| WithheldCandidate {
            beta_sq: c64::new(w.pair.lambda, 0.0),
            residual: w.residual,
            curl_ratio: curl_ratio(&w.pair.vector),
            eps_weighted: Some(eps_weighted(&w.pair.vector)),
        })
        .collect();
    let withheld_in_window = checked
        .rejected
        .iter()
        .filter(|(lambda, _)| guided_window.0 < *lambda && *lambda < guided_window.1)
        .count();
    let lanczos_steps = checked.lanczos_steps;
    let mut cands: Vec<RawDielectricCandidate> = checked
        .pairs
        .into_iter()
        .zip(checked.residuals)
        .map(|(pair, residual)| RawDielectricCandidate {
            beta_sq: pair.lambda,
            curl_ratio: curl_ratio(&pair.vector),
            eps_weighted: eps_weighted(&pair.vector),
            residual,
            vector: pair.vector,
        })
        .collect();
    cands.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    RawDielectricSolve {
        cands,
        withheld,
        withheld_in_window,
        localized_withheld,
        sigma,
        lanczos_steps,
    }
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
) -> Result<RawDielectricSolve, EigenError> {
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
        return Ok(RawDielectricSolve {
            cands: Vec::new(),
            withheld: 0,
            withheld_in_window: 0,
            localized_withheld: Vec::new(),
            sigma: 0.0,
            lanczos_steps: 0,
        });
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
    // ⟨ε⟩_x = (xᵀ M_ε x)/(xᵀ M₁ x), the field-weighted permittivity.
    let eps_weighted = |x_interior: &[f64]| -> f64 {
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
        let xm1x = sparse_quadratic_form(m1_sparse.as_ref(), x_interior);
        xmx / xm1x.abs().max(1e-300)
    };

    // The guided β² window the classifier keeps (`n_clad²k₀²`, ceiling²k₀²):
    // only unconverged pairs inside it are worth extending the run for.
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_eff_ceiling = n_eff_target.unwrap_or(n_core).min(n_core);
    let guided_window = (eps_min * k0_sq, n_eff_ceiling * n_eff_ceiling * k0_sq);
    let checked = dielectric_checked_eigenpairs(
        a_sparse.as_ref(),
        m1_sparse.as_ref(),
        sigma,
        n_request,
        dim,
        guided_window,
    )?;
    Ok(raw_dielectric_solve(
        checked,
        sigma,
        guided_window,
        curl_ratio,
        eps_weighted,
    ))
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
///
/// # Contrast scaling (issue #794)
///
/// The sweep above is Si/SiO₂ only. The curl ratio of any in-window
/// eigenpair is bounded by the index contrast, `r < (ε_max − ε_min)/ε_max`
/// (derivation on [`physical_curl_floor_p2`]; the identity is
/// order-agnostic). For SMF-28 that bound is `7.8×10⁻³`, so a fixed `3×10⁻²`
/// rejected every guided mode of a weakly-guiding cross-section. The p=1
/// floor therefore uses the same contrast scaling as p=2 (#791):
///
/// ```text
///   floor = clamp( GUIDED_CURL_FLOOR_FRACTION · (ε_max − ε_min)/ε_min,
///                  physical_curl_floor_pml() = 1e-6,
///                  HIGH_CONTRAST_CURL_FLOOR = 3e-2 ).
/// ```
///
/// The cap is reached at `(ε_max − ε_min)/ε_min = 3`, so every Si/SiO₂
/// cross-section (contrast ≈ 4.7), which is every current p=1 caller, keeps
/// exactly the calibrated `3×10⁻²` above. The floor still depends only on
/// the materials. At intermediate contrast (SiN/SiO₂, floor ≈ `9×10⁻³`) the
/// p=1 near-ceiling spurious band has not been swept. Use p=2 there, where
/// `p2_dielectric_solve_intermediate_contrast_sin_strip` pins the behavior.
fn physical_curl_floor(eps_max: f64, eps_min: f64) -> f64 {
    let contrast = if eps_min > 0.0 {
        ((eps_max - eps_min) / eps_min).max(0.0)
    } else {
        f64::INFINITY
    };
    (GUIDED_CURL_FLOOR_FRACTION * contrast)
        .clamp(physical_curl_floor_pml(), HIGH_CONTRAST_CURL_FLOOR)
}

// ===========================================================================
// Phase-2.5C (Epic #318): order-aware (p=2) dielectric eigensolver
// ===========================================================================

/// Curl-energy floor separating the physical guided band from the
/// gradient-nullspace band for the **p=2** Nédélec pencil, scaled to the
/// index contrast of the cross-section (issue #791).
///
/// Returns
///
/// ```text
///   floor = clamp( GUIDED_CURL_FLOOR_FRACTION · (ε_max − ε_min)/ε_min,
///                  physical_curl_floor_pml() = 1e-6,
///                  HIGH_CONTRAST_CURL_FLOOR = 3e-2 ).
/// ```
///
/// This is the same formula as the p=1 [`physical_curl_floor`] (issue #794);
/// the two orders share the high-contrast cap and the scaling.
///
/// # The high-contrast cap at p=2
///
/// The p=1 cap (`3e-2`) is calibrated against the
/// measured curl-energy gap of the *Whitney* pencil. At p=2 the pencil's
/// gradient nullspace is larger (it gains the `Q = ∇(λ_aλ_b)` edge DOFs and
/// interior modes — see [`spurious_dim_2d_p2`]), but those gradient modes
/// are still *exactly* curl-free by construction (`∇×Q ≡ 0`,
/// `∇×∇φ ≡ 0`): a converged p=2 gradient eigenpair has `r = (xᵀKx)/(k₀²xᵀM_εx)`
/// at the f64 noise floor (`≈ 10⁻¹⁶`), the same as p=1. A p=2 curl-energy
/// refinement sweep (rect TE-cavity and the SOI strip at ny ∈ {20,30,40,60})
/// shows the p=2 genuine guided band floor at `r ≳ 1.2×10⁻¹` — above the
/// p=1 genuine floor (`≈ 8.5×10⁻²`) — so for those **high-contrast**
/// (Si/SiO₂) cross-sections the p=1 value `3×10⁻²` sits inside the p=2 gap.
/// That calibrated value is kept, unchanged, as the cap
/// [`HIGH_CONTRAST_CURL_FLOOR`].
///
/// # Why a fixed `3e-2` is wrong for weak contrast (issue #791)
///
/// The curl energy an eigenpair of the pencil can carry is bounded by the
/// index contrast, **exactly**. Multiplying `A x = β² M₁ x` (with
/// `A = k₀²M_ε − K`) by `xᵀ` gives the Rayleigh identity
///
/// ```text
///   r = (xᵀKx)/(k₀² xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x,   ⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x).
/// ```
///
/// `⟨ε⟩_x` is a field-weighted average of `ε_r`, so `⟨ε⟩_x ≤ ε_max`, and
/// a guided pair has `n_eff² > ε_min`. Every in-window eigenpair therefore
/// obeys
///
/// ```text
///   r < (ε_max − ε_min) / ε_max,
/// ```
///
/// on the discrete pencil and at any contrast (see
/// [`rayleigh_consistent`], which enforces it).
///
/// For SMF-28 (`n_core = 1.4504`, `n_clad = 1.4447`) that bound is
/// `7.8×10⁻³`. The old fixed `3×10⁻²` floor sat **3.8× above the largest
/// curl ratio any guided eigenpair can have**, so the solver could never
/// return a genuine guided mode of a weakly-guiding fiber. The only
/// candidate that passed (`n_eff = 1.445373`, `r = 3.3×10⁻²`, core-energy
/// fraction 0.21) was therefore **not an eigenpair at all**. It was an
/// unconverged Ritz vector from the tail of the shift-invert Lanczos window
/// (`max_iters = n_request + 8`): its `r` would need `⟨ε⟩_x = 2.161`, above
/// `ε_core = 2.104`. Whether that tail vector appears depends on
/// floating-point rounding (present on aarch64 macOS, absent on x86_64
/// Linux), so the solve returned 1 mode on one platform, 0 on the other,
/// and 0 on a refined mesh.
///
/// # The calibration
///
/// The floor is scaled by `(ε_max − ε_min)/ε_min` rather than by the exact
/// bound's `/ε_max`. The two differ by the factor `ε_max/ε_min`, which is
/// `1.008` for SMF-28 and `1.06` for the ~3 %-step fiber. At high contrast
/// the floor is capped anyway. In every case the floor stays below
/// `0.04 ×` the exact bound: for `(ε_max − ε_min)/ε_min = c ≤ 3` the ratio
/// is `10⁻²(1 + c) ≤ 0.04`, and above the cap the bound exceeds `0.75`.
///
/// The measured guided fundamentals sit at `r ≈ 0.09 ×` the exact bound on
/// both audit fibers (SMF-28 `r = 7.2×10⁻⁴` against `7.8×10⁻³`; the
/// ~3 %-step fiber `r = 5.0×10⁻³` against `5.7×10⁻²`).
/// [`GUIDED_CURL_FLOOR_FRACTION`] `= 10⁻²` places the floor about 8–9×
/// below the guided fundamentals and still at least ten decades above
/// the exactly curl-free gradient nullspace (`r ≈ 10⁻¹⁶`). The lower clamp
/// `10⁻⁶` is the PML path's nullspace floor ([`physical_curl_floor_pml`]),
/// so a near-zero-contrast input still rejects the nullspace. The upper
/// clamp is reached at `(ε_max − ε_min)/ε_min = 3`, so every Si/SiO₂
/// cross-section (≈ 4.7) keeps exactly the calibrated `3×10⁻²`, and its
/// weakly-resolved near-ceiling spurious band (`r ≈ 3×10⁻³…1.7×10⁻²`)
/// stays rejected. Those spurious pairs are converged eigenpairs of the
/// discrete pencil, so the Rayleigh check does not remove them. At
/// intermediate contrast (SiN/SiO₂, floor `≈ 9×10⁻³`) the
/// [`physical_index_ceiling`] window removes near-ceiling spurious pairs of
/// 2-D-confined cores, and the unit test
/// `p2_dielectric_solve_intermediate_contrast_sin_slab` pins the slab case.
///
/// The floor depends only on the materials, never on the candidate
/// spectrum, so an out-of-window spike cannot raise it (the same argument
/// as for p=1).
fn physical_curl_floor_p2(eps_max: f64, eps_min: f64) -> f64 {
    physical_curl_floor(eps_max, eps_min)
}

/// Upper clamp of [`physical_curl_floor`] and [`physical_curl_floor_p2`]:
/// the high-contrast (Si/SiO₂) curl floor, calibrated for p=1 by the
/// refinement sweep on [`physical_curl_floor`] and confirmed for p=2 in
/// Epic #318 Phase 2.5C.
const HIGH_CONTRAST_CURL_FLOOR: f64 = 3e-2;

/// Fraction of the contrast `(ε_max − ε_min)/ε_min` used as the curl floor
/// at both orders (issues #791, #794). See [`physical_curl_floor_p2`].
const GUIDED_CURL_FLOOR_FRACTION: f64 = 1e-2;

/// Order-aware (p=2) sibling of [`solve_dielectric_modes`]: solve the
/// dielectric full-vector transverse-mode eigenproblem using the
/// **second-order** Nédélec assembly ([`assemble_2d_nedelec2_with_epsilon`])
/// and the p=2 interior-DOF mask (e.g. [`rect_pec_interior_dofs2`] /
/// [`disk_pec_interior_dofs2`]).
///
/// The eigenpencil `A = k₀²M_ε − K`, `A x = β² M₁ x`, the sparse
/// shift-invert Lanczos path, the guided-band shift target, the
/// bound-window classifier, the `physical_index_ceiling` geometry ceiling,
/// and the `pin_eigenvector_sign` gauge are all **order-agnostic** and
/// reused verbatim; only the assembly order and the curl-energy floor
/// (`physical_curl_floor_p2`, which scales with the index contrast so a
/// weakly-guiding fiber's guided modes are not rejected; issue #791)
/// differ. Returns up to `n_modes` guided
/// [`DielectricMode`]s ordered fundamental-first (largest `n_eff`), with
/// `e_edges` in the **p=2 DOF ordering** (length [`n_dof_2d_nedelec2`]).
///
/// Both orders share the residual-checked Lanczos solve (issue #798), the
/// contrast-scaled curl floor (issues #791, #794) and the Rayleigh-identity
/// check `rayleigh_consistent`.
///
/// # Errors
///
/// Returns [`EigenError`] if the sparse eigensolve fails. Returns an empty
/// `Vec` (not an error) if no bound modes exist in the window.
/// Returns [`EigenError::SelectionHole`] if a localized, bound-like withheld
/// Ritz pair still sits above the lowest returned mode after one automatic
/// retry with a doubled Lanczos request (issue #850; see `selection_hole`).
/// When the solve finds **no** bound mode,
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
pub fn solve_dielectric_modes2(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    let n_request = (n_modes + 8).max(16);
    retry_on_selection_hole("solve_dielectric_modes2", n_request, |n_request| {
        solve_dielectric_modes2_attempt(mesh, eps_r, interior_dof_mask, k0, n_modes, n_request)
    })
}

/// One [`solve_dielectric_modes2`] solve + classification at a given
/// Lanczos request (see [`solve_dielectric_modes_attempt`]).
fn solve_dielectric_modes2_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricMode>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    let index_ceiling = physical_index_ceiling(mesh, eps_r, k0);
    let n_eff_ceiling = index_ceiling.unwrap_or(n_core);
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let raw =
        dielectric_raw_candidates_p2(mesh, eps_r, interior_dof_mask, k0, n_request, index_ceiling)?;
    let cands = &raw.cands;
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_p2(eps_max, eps_min);

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    let mut bound: Vec<DielectricMode> = Vec::new();
    let mut n_dropped = 0usize;
    let mut n_inconsistent = 0usize;
    for c in cands {
        debug_assert!(c.residual <= DIELECTRIC_RESIDUAL_TOL);
        let in_window = c.beta_sq > beta_sq_floor && c.beta_sq < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        // Issue #791: an in-window pair whose curl ratio breaks the exact
        // Rayleigh identity cannot be an eigenpair of the pencil. Since #798
        // the residual check withholds unconverged Ritz pairs first; this is
        // the cheap second line.
        if !rayleigh_consistent(c, k0, eps_max, eps_min) {
            n_dropped += 1;
            n_inconsistent += 1;
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
         dropped {n_dropped} radiation/spurious eigenpair(s), {n_inconsistent} of \
         them failing the Rayleigh identity; withheld {} unconverged Ritz pair(s) \
         (residual > {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps), {} of them \
         in the guided window (requested {n_modes})",
        raw.withheld, raw.lanczos_steps, raw.withheld_in_window
    );
    // Selection-level hole check (issues #850, #913), as in
    // `solve_dielectric_modes`.
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole_real(
            "solve_dielectric_modes2",
            &raw,
            (beta_sq_floor, beta_sq_ceiling),
            bound.last().map(|m| m.beta_sq),
            &HoleRule::new(curl_floor, eps_max, eps_min, Some(k0)),
        )
    };
    Ok((bound, hole))
}

/// Relative tolerance of the Rayleigh-identity check, as a fraction of
/// the guided curl bound `(ε_max − ε_min)/ε_max` (issue #791). See
/// [`rayleigh_consistent`].
const RAYLEIGH_IDENTITY_TOL_FRACTION: f64 = 1e-3;

/// Violation `δ = |r − (1 − β²/(k₀²⟨ε⟩_x))|` of the exact Rayleigh identity
/// of the pencil `A = k₀²M_ε − K`, `A x = β² M₁ x` (issue #791).
///
/// Multiplying the eigen-equation by `xᵀ` gives
/// `β² xᵀM₁x = k₀² xᵀM_εx − xᵀKx`. Dividing by `k₀² xᵀM_εx`,
///
/// ```text
///   r = (xᵀKx)/(k₀² xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x,   ⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x),
/// ```
///
/// so every exact eigenpair has `δ = 0`. For an approximate pair,
/// `δ·⟨ε⟩_x = |n_RQ² − n_eff²|`: the gap between the reported eigenvalue
/// and the Rayleigh quotient of the returned vector, in units of `n_eff²`.
fn rayleigh_identity_violation(c: &RawDielectricCandidate, k0: f64) -> f64 {
    rayleigh_identity_violation_of(c.beta_sq, c.curl_ratio, c.eps_weighted, k0)
}

/// [`rayleigh_identity_violation`] on the bare `(β², r, ⟨ε⟩_x)` triple, so
/// the hole check can apply it to a withheld pair (issue #913).
fn rayleigh_identity_violation_of(
    beta_sq: f64,
    curl_ratio: f64,
    eps_weighted: f64,
    k0: f64,
) -> f64 {
    let eps_x = eps_weighted.max(f64::MIN_POSITIVE);
    (curl_ratio - (1.0 - beta_sq / (k0 * k0 * eps_x))).abs()
}

/// Whether an **in-window** candidate (p=1 or p=2) can be an eigenpair of
/// the pencil `A x = β² M₁ x`, by the exact Rayleigh identity (issue #791).
///
/// Every eigenpair satisfies `r = 1 − n_eff²/⟨ε⟩_x` (derivation on
/// [`rayleigh_identity_violation`]). The field-weighted permittivity
/// `⟨ε⟩_x` is a convex average of the per-triangle `ε_r`, so it lies in
/// `[ε_min, ε_max]`. A guided pair has `n_eff² > ε_min`, so
///
/// ```text
///   r ≤ 1 − n_eff²/ε_max < 1 − ε_min/ε_max = (ε_max − ε_min)/ε_max.
/// ```
///
/// This is exact for the discrete pencil at any contrast; it needs no
/// weak-guidance approximation. A candidate is rejected when either
///
/// - `r ≥ (ε_max − ε_min)/ε_max`, which no in-window eigenpair can reach
///   (this test needs no `⟨ε⟩_x`), or
/// - `δ > RAYLEIGH_IDENTITY_TOL_FRACTION · (ε_max − ε_min)/ε_max`.
///
/// # The tolerance
///
/// `δ·⟨ε⟩_x` is the disagreement in `n_eff²` between the reported
/// eigenvalue and the returned field, and `ε_max − ε_min` is the guided
/// window in `n_eff²`. Since `⟨ε⟩_x ≤ ε_max`, the rule rejects a pair whose
/// two estimates of `n_eff²` differ by more than `10⁻³` of the window,
/// that is, by more than `10⁻³` in normalized `b`. That is ten times finer
/// than the 1 % `b` accuracy the fiber benchmarks claim. On the #449/#791
/// audit fibers the converged Lanczos pairs give `δ ≈ 10⁻¹⁴…10⁻¹⁰`, more
/// than four decades below the threshold (`7.8×10⁻⁶` for SMF-28). The
/// unconverged tail pairs that the pre-#791 selection returned give
/// `δ = 3.3×10⁻²` (SMF-28, `r = 3.3×10⁻²`) and `δ = 0.39` (~3 %-step
/// fiber, `r = 0.38`), and both also break the `r` bound outright.
///
/// # A second line, not the convergence test (issue #798)
///
/// `δ` is second order in the pair's residual `ρ` (`δ ≈ ρ²` on the
/// calibration fixtures), so this check only fires for `ρ ≳ 3×10⁻³`. Pairs
/// with `ρ ≈ 10⁻²` can pass it. The convergence test is the residual check
/// in the eigensolve ([`DIELECTRIC_RESIDUAL_TOL`]); this check is kept as a
/// cheap, independent second line at both orders.
fn rayleigh_consistent(c: &RawDielectricCandidate, k0: f64, eps_max: f64, eps_min: f64) -> bool {
    rayleigh_consistent_of(
        rayleigh_identity_violation(c, k0),
        c.curl_ratio,
        eps_max,
        eps_min,
    )
}

/// [`rayleigh_consistent`] on a pair's identity violation `δ` and curl
/// ratio `r`, so the hole check can apply the classifier's own test to a
/// withheld pair (issue #913).
fn rayleigh_consistent_of(violation: f64, curl_ratio: f64, eps_max: f64, eps_min: f64) -> bool {
    let bound = if eps_max > 0.0 {
        ((eps_max - eps_min) / eps_max).max(0.0)
    } else {
        0.0
    };
    if curl_ratio >= bound {
        return false;
    }
    violation <= RAYLEIGH_IDENTITY_TOL_FRACTION * bound
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
/// recovery branch for the complex PML pencil.
///
/// Delegates to the cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`] (issue #831), which has the
/// same branch: `Re √z ≥ 0`, `sign Im √z = sign Im z` (`Im z = ±0` counts as
/// non-negative, so a real negative `z` maps to `+i√|z|`), and `0 → 0`. The
/// old local copy computed `Im √z = √(½(|z| − Re z))`, which cancels: for a
/// weakly leaky guided mode its relative error in `Im β` (and so in the
/// propagation loss `Im n_eff`) was `≈ ε · (Re β² / Im β²)²`, and it returned
/// `Im β = 0` (lossless) once `Im β² / Re β² < √ε ≈ 1.5e-8`.
fn principal_sqrt_c64(z: c64) -> c64 {
    crate::eigen::wavenumber::principal_sqrt(z)
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
) -> Result<RawDielectricSolvePml, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
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
        return Ok(RawDielectricSolvePml {
            cands: Vec::new(),
            localized_withheld: Vec::new(),
            sigma: 0.0,
            lanczos_steps: 0,
        });
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

    // Converged pairs only (issue #834): the complex bilinear Lanczos has
    // no interlacing guarantee, so an unconverged Ritz value can land in
    // the guided window, even nearer σ than converged modes.
    let guided_window = (eps_min * k0_sq, beta_sq_ceiling);
    let checked = pml_checked_eigenpairs(
        a_sparse.as_ref(),
        m1_sparse.as_ref(),
        sigma,
        n_request,
        dim,
        guided_window,
    )?;
    log_pml_withheld("dielectric_raw_candidates_p2_pml", &checked, guided_window);
    // Issue #916: curl ratio of each localized withheld Ritz vector.
    let localized_withheld: Vec<WithheldCandidate> = checked
        .localized_withheld
        .iter()
        .map(|w| WithheldCandidate {
            beta_sq: w.pair.lambda,
            residual: w.residual,
            curl_ratio: curl_ratio(&w.pair.vector),
            eps_weighted: None,
        })
        .collect();
    let lanczos_steps = checked.lanczos_steps;
    let pairs = checked.pairs;

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
    Ok(RawDielectricSolvePml {
        cands,
        localized_withheld,
        sigma,
        lanczos_steps,
    })
}

/// The converged candidates of one PML / complex dielectric pencil solve
/// ([`dielectric_raw_candidates_p2_pml`]) plus the localized withheld pairs
/// the classifiers test for a hole in their selection (issue #850).
struct RawDielectricSolvePml {
    /// Converged candidates, sorted by decreasing `Re β²`.
    cands: Vec<RawDielectricCandidateComplex>,
    /// Localized withheld pairs with their curl ratios
    /// ([`CheckedComplexEigenpairs::localized_withheld`], issue #916).
    localized_withheld: Vec<WithheldCandidate>,
    /// Shift `σ` of the solve.
    sigma: f64,
    /// Lanczos steps run (first pass plus any extension).
    lanczos_steps: usize,
}

/// Relative leakage `|Im β²| / Re β²` at or below which a guided-window
/// pair counts as **bound** (the PML classifiers' bound/leaky cut; see
/// [`solve_dielectric_modes2_pml`]). Relative in every mesh length unit
/// (issue #828).
const DIELECTRIC_BOUND_REL_IM: f64 = 1e-8;

/// A **localized** withheld Ritz pair of a dielectric solve, as the
/// selection-hole check sees it (issues #850, #916).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WithheldCandidate {
    /// Ritz value `β²` (real on the PEC path, so `Im β² = 0`).
    pub beta_sq: c64,
    /// Relative true residual `ρ` of the pair (above
    /// [`DIELECTRIC_RESIDUAL_TOL`]).
    pub residual: f64,
    /// Relative curl energy of the Ritz vector, computed exactly as the
    /// classifier computes it for a converged candidate
    /// ([`RawDielectricCandidate::curl_ratio`]): `≈ 0` for a
    /// gradient-nullspace pair.
    pub curl_ratio: f64,
    /// Field-weighted permittivity `⟨ε⟩_x` of the Ritz vector
    /// ([`RawDielectricCandidate::eps_weighted`]) on the real (PEC) path,
    /// where the classifiers apply the Rayleigh-identity test
    /// ([`rayleigh_consistent`]) to their converged candidates. `None` on
    /// the complex (PML) path, whose classifiers apply no such test. Used
    /// only by the empty-bound-set rule (issue #913).
    pub eps_weighted: Option<f64>,
}

/// Selection-level **hole** rule of the dielectric bound-mode classifiers
/// (issue #850, the follow-up to PR #847).
///
/// Returns the withheld pair with the largest `Re β²` that
///
/// 1. is **localized** (already filtered: `localized_withheld` comes from
///    [`CheckedEigenpairs::localized_withheld`] /
///    [`CheckedComplexEigenpairs::localized_withheld`], the solver's own
///    `ρ · max(|λ|, |σ|) ≤ |λ − σ|` test, so it locates a genuine
///    eigenvalue that was still unconverged at the Lanczos cap),
/// 2. carries **curl energy**, `curl_ratio > curl_floor`, where
///    `curl_floor` is the floor the calling classifier applies to its
///    converged candidates (issue #916). A curl-free gradient-nullspace
///    pair could never have been selected (the classifier drops it), so
///    withholding it leaves no hole. In this pencil the gradient cluster
///    is spread across the whole guided band (see
///    [`solve_dielectric_modes`]), so without this test such a pair above
///    the reference would trip a false [`EigenError::SelectionHole`],
/// 3. lies inside the classifier's guided window **with certainty**,
///    `guided_window.0 + w < Re β² < guided_window.1 − w`,
/// 4. is **bound-like**, `|Im β²| ≤ DIELECTRIC_BOUND_REL_IM · Re β²`
///    (trivially true on the real path), and
/// 5. has `Re β²` **certainly above** `lowest_returned_bound`, the lowest
///    `Re β²` among the bound modes the classifier is about to return:
///    `Re β² − w > lowest_returned_bound`.
///
/// When the classifier returns **no** bound mode (`lowest_returned_bound =
/// None`) condition 5 has no reference; the **empty-bound-set** conditions
/// replace it (issue #913, below).
///
/// `w = ρ · max(|β²|, |σ|)` is the pair's first-order eigenvalue
/// uncertainty, the same bound the localization test uses. Without it the
/// curl-free gradient cluster that sits at `β² = ε_core k₀²` (the PML
/// window ceiling) to rounding would count as in-window: on the coarse
/// `(3, 32)` high-contrast PML mesh a withheld copy at `36.35397`
/// (`ρ = 5.9×10⁻⁸`) falls below the ceiling by `~10⁻¹⁴`. Requiring a
/// certain margin also keeps a ghost copy of the lowest returned mode
/// from counting as a pair above it.
///
/// Such a pair would have ranked inside the returned bound set (the
/// classifiers order bound modes by descending `Re β²`), so returning
/// without it leaves a hole, and it could be the true fundamental.
///
/// # The empty-bound-set rule (issue #913)
///
/// A classifier that found no bound mode used to pass unchecked, because
/// the rule had no reference mode to compare a withheld pair against. With
/// `lowest_returned_bound = None`, a pair meeting conditions 1–4 is now
/// **reported** (this function returns it) when it also
///
/// - is **resolved**, `ρ ≤` [`EMPTY_SET_RESIDUAL_CAP`] `= 10⁻⁴`,
/// - has a curl ratio above [`HoleRule::empty_set_curl_floor`], which is
///   [`EMPTY_SET_CURL_MARGIN`] `= 2` times the contrast-scaled
///   [`physical_curl_floor`] on every path, and
/// - on the real path, passes the classifier's own Rayleigh-identity test
///   ([`rayleigh_consistent`]).
///
/// These are stricter than conditions 1–4 on purpose. With a reference
/// mode, the question is whether the classifier *would have selected* the
/// pair, and its own floor answers that. With none, a report says that a
/// bound mode may exist and was missed, on the evidence of one unconverged
/// vector. The extra conditions keep the report to pairs whose diagnostics
/// are resolved and sit clear of the low-curl ladders measured on
/// [`EMPTY_SET_CURL_MARGIN`], [`EMPTY_SET_RESIDUAL_CAP`] and
/// [`HoleRule::empty_set_curl_floor`]. A pair one of them excludes is
/// labelled in the hole-check log line, not dropped silently.
///
/// ## Warn-only
///
/// A reported pair is **not** an error and triggers no retry:
/// [`check_selection_hole`] logs [`empty_set_warning`] and the classifier
/// returns its (empty) set, exactly as before #913. The with-reference
/// rule above is unaffected and still goes through
/// [`retry_on_selection_hole`].
///
/// The reason is the evidence. The one natural firing measured is the
/// thin-core PML fixture of
/// `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01`
/// (`a = 0.30 µm`, `n = 1.4874 / 1.4447`, `V ≈ 0.43`), and the pair it
/// reports is not a guided mode. The analytic LP₀₁ of that fiber has
/// `b = 4.7×10⁻⁹`; the reported pair has `b ≈ 0.42` and, as measured in
/// the review of PR #948, moves with radial
/// refinement (`b = 0.417, 0.468, 0.529` on meshes `(4, 48)`, `(6, 64)`,
/// `(16, 96)`) and with the PML box radius (`0.407, 0.417, 0.307` for a
/// cladding radius of `4, 8, 16 µm`), and tracks its own core energy
/// fraction (`β² ≈ k₀²⟨ε⟩`). It is a member of the low-curl ladder that
/// the PML classifier returns as bound modes once it converges (issue
/// #947), and it clears the curl threshold by `1.33×`. The correct result
/// for that fiber at any resolvable precision is the empty set, which an
/// error-after-retry policy would have replaced by the artifact or by an
/// error. The curl ladder is continuous, so no margin separates it
/// cleanly.
///
/// Error-after-retry can return for this case once a committed positive
/// shows a reported pair whose `β²` matches an analytic guided mode within
/// a stated tolerance, and the thin-core pair is shown not to fire.
///
/// A window with no interior (`ε_max = ε_min`) admits no pair, so a
/// uniform cross-section is never a hole.
///
/// # Why not "nearer `σ` than a returned mode"
///
/// That is the PR #847 rule of [`crate::eigen::lossy_cavity`]. The guided
/// window here is crowded with leaky / PML-continuum pairs that
/// legitimately stay unconverged at the cap, so a withheld localized pair
/// nearer `σ` than some returned continuum mode is normal. The
/// high-contrast fiber fixture withholds one at `β² = 34.94`, *below* its
/// `35.8` fundamental; under this rule it is not a hole.
///
/// The curl ratio of a withheld pair is that of its Ritz vector, which is
/// not converged. The curl ratio is a quadratic form, so the error a small
/// eigenvector perturbation `δ` makes in it is `O(‖δ‖²)` for a curl-free
/// pair: a localized gradient pair (`ρ ≪ 1`) stays near f64 noise, far
/// below every floor, while a physical pair keeps a ratio in the physical
/// band. A Ritz vector that mixes the two carries the physical part's curl
/// and is still checked. This is conservative for gradient pairs, but not
/// strictly so for a mostly-gradient mixed vector: below roughly 20 %
/// physical weight its curl ratio can fall under the PEC floor and the pair
/// is filtered, the same ambiguity a converged pair has.
fn selection_hole(
    localized_withheld: &[WithheldCandidate],
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> Option<WithheldCandidate> {
    localized_withheld
        .iter()
        .copied()
        .filter(|c| {
            hole_verdict(c, sigma, guided_window, lowest_returned_bound, rule) == HoleVerdict::Hole
        })
        .max_by(|a, b| a.beta_sq.re.total_cmp(&b.beta_sq.re))
}

/// Largest relative residual `ρ` at which the **empty-bound-set** rule
/// reports a withheld pair (issue #913; see [`selection_hole`]).
///
/// With no returned bound mode the rule has nothing to compare the pair
/// against, so it rests on the pair's own diagnostics, and those have to be
/// resolved. The eigenvalue error of a Ritz pair is second order in its
/// residual, so `Im β²` of a truly bound pair is only known to about
/// `ρ² · Re β²`. The bound cut is [`DIELECTRIC_BOUND_REL_IM`] `= 10⁻⁸`
/// relative, so the bound-like test is resolved for `ρ ≤ √10⁻⁸ = 10⁻⁴`.
/// On the real (PEC) path `Im β² = 0` identically; there the cap keeps the
/// Rayleigh violation (`δ ≈ ρ²`, see [`rayleigh_consistent`]) and the curl
/// ratio of the Ritz vector near their converged values.
///
/// The bound is conservative. On the high-contrast PML fixture the
/// withheld pairs of the near-cladding ladder read
/// `|Im β²| = 2×10⁻⁸, 1.6×10⁻⁸, 1.3×10⁻⁶` at `ρ = 4.2×10⁻⁴, 6.8×10⁻⁴,
/// 4.9×10⁻³`, about `10⁻³` of `ρ² · Re β²`, and reach the bound cut
/// (`3.5×10⁻⁷`) between the last two. The cap also drops reports: on the
/// thin-core fixture of
/// `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01` (mesh
/// `(6, 64)`, requests 7 and 8) the same ladder pair the rule reports at
/// `ρ ≤ 2×10⁻⁶` is withheld at `ρ = 1.19×10⁻⁴` and only listed in the log.
const EMPTY_SET_RESIDUAL_CAP: f64 = 1e-4;

/// Margin of the empty-bound-set curl threshold over the contrast-scaled
/// curl floor [`physical_curl_floor`] (issue #913; see [`HoleRule`]).
///
/// The classifier's own floor is not a wide-gap separator once the
/// gradient nullspace is excluded. Measured on the ~3 %-step PEC fiber of
/// `formulation_audit_graddiv` (p=2, request 16, floor `6.00×10⁻⁴`), the
/// localized withheld pairs form a continuous low-curl ladder:
/// `2.89×10⁻³` (the genuine `β² = 35.385` twin, `4.8×` the floor), then
/// `6.06×10⁻⁴` (`1.01×`), `4.75×10⁻⁴`, `3.85×10⁻⁴`, `3.23×10⁻⁴`. With a
/// reference mode the `1.01×` pair is harmless (it sits below the
/// reference); with none it would decide the report on a 1 % margin.
///
/// The floor is calibrated 8–9× below the guided fundamentals of both
/// audit fibers and 2.8× below the genuine band of the Si/SiO₂ sweep
/// (`8.5×10⁻²` against `3×10⁻²`; see [`physical_curl_floor`] and
/// [`physical_curl_floor_p2`]). A factor 2 clears the near-floor ladder
/// and stays below every calibrated genuine band.
///
/// It does not clear the ladder everywhere: the thin-core PML pair of
/// issue #947 sits at `1.33×` this threshold and is reported. That is one
/// reason the rule only warns (see [`selection_hole`]).
const EMPTY_SET_CURL_MARGIN: f64 = 2.0;

/// The thresholds a classifier hands the selection-hole check
/// ([`selection_hole`], issues #850, #916, #913).
#[derive(Debug, Clone, Copy, PartialEq)]
struct HoleRule {
    /// The curl floor the classifier applies to its converged candidates
    /// (issue #916): a withheld pair at or below it is never a hole.
    curl_floor: f64,
    /// Curl threshold of the **empty-bound-set** rule (issue #913):
    /// `EMPTY_SET_CURL_MARGIN ·` [`physical_curl_floor`]`(ε_max, ε_min)`,
    /// and never below [`Self::curl_floor`].
    ///
    /// It is contrast-scaled on **every** path, the PML path included. The
    /// PML classifiers' own floor ([`physical_curl_floor_pml`] `= 10⁻⁶`)
    /// only rejects the gradient nullspace. Their bound-like population
    /// also holds a low-curl ladder of PML-box pairs above it: measured
    /// `17…127×` that floor on the SMF-28 PML fixtures and `139…1160×` on
    /// the high-contrast ones (pairs below this threshold only), and
    /// `6…8×` (weak, SMF-28 contrast) and `63…119×` (~3 % contrast) on
    /// two **anti-guides**, which guide nothing. Against this threshold the
    /// anti-guide ladders read `0.04…0.05×` and `0.05…0.10×`. Those ladder
    /// pairs are bound modes to the PML classifier when they converge (it
    /// returns them at a larger request, anti-guide included; issue #947),
    /// so this threshold is deliberately stricter than the classifier: with
    /// no reference mode a report needs evidence of a physical guided
    /// mode, and the classifier's nullspace floor is not that.
    empty_set_curl_floor: f64,
    /// `(k₀, ε_max, ε_min)` of the classifier's Rayleigh-identity test
    /// ([`rayleigh_consistent`]) on the real (PEC) path; `None` on the PML
    /// path, whose classifiers apply none. The empty-bound-set rule applies
    /// the same test to the withheld pair.
    rayleigh: Option<(f64, f64, f64)>,
}

impl HoleRule {
    /// Rule of a classifier with curl floor `curl_floor` on a cross-section
    /// with permittivity range `[eps_min, eps_max]`. `rayleigh_k0` is
    /// `Some(k₀)` for the real-path classifiers (which apply
    /// [`rayleigh_consistent`]) and `None` for the PML ones.
    fn new(curl_floor: f64, eps_max: f64, eps_min: f64, rayleigh_k0: Option<f64>) -> Self {
        Self {
            curl_floor,
            empty_set_curl_floor: (EMPTY_SET_CURL_MARGIN * physical_curl_floor(eps_max, eps_min))
                .max(curl_floor),
            rayleigh: rayleigh_k0.map(|k0| (k0, eps_max, eps_min)),
        }
    }
}

/// How the hole rule ([`selection_hole`]) classifies one localized withheld
/// pair. Every variant but [`Self::Hole`] and [`Self::NotCandidate`] names
/// the single test that excluded the pair, so the hole-check log line can
/// say why a pair was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoleVerdict {
    /// The pair meets every condition. With a reference mode it is a hole
    /// (an error after one retry). With none it is an empty-set report: a
    /// warning only (issue #913).
    Hole,
    /// Outside the certain window, not bound-like, or (with a reference)
    /// not certainly above it.
    NotCandidate,
    /// Curl ratio at or below the classifier's floor (issue #916).
    BelowCurlFloor,
    /// Empty-set rule only: `ρ >` [`EMPTY_SET_RESIDUAL_CAP`].
    Unresolved,
    /// Empty-set rule only: curl ratio at or below
    /// [`HoleRule::empty_set_curl_floor`].
    BelowEmptySetCurl,
    /// Empty-set rule only: fails the classifier's [`rayleigh_consistent`]
    /// test.
    RayleighInconsistent,
}

impl HoleVerdict {
    /// Label appended to the pair in the hole-check log line.
    fn label(self) -> &'static str {
        match self {
            Self::Hole | Self::NotCandidate => "",
            Self::BelowCurlFloor => ", at/below curl floor: skipped",
            Self::Unresolved => ", ρ above the empty-set cap: not counted",
            Self::BelowEmptySetCurl => ", curl at/below the empty-set threshold: not counted",
            Self::RayleighInconsistent => ", breaks the Rayleigh identity: not counted",
        }
    }
}

/// Classify one localized withheld pair under the hole rule; see
/// [`selection_hole`] for the conditions.
fn hole_verdict(
    c: &WithheldCandidate,
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> HoleVerdict {
    let l = c.beta_sq;
    let w = c.residual * l.norm().max(sigma.abs());
    // Positive comparisons throughout, so a NaN diagnostic is never a hole.
    let has_curl = c.curl_ratio > rule.curl_floor;
    if !has_curl {
        return HoleVerdict::BelowCurlFloor;
    }
    let candidate = guided_window.0 + w < l.re
        && l.re < guided_window.1 - w
        && l.im.abs() <= DIELECTRIC_BOUND_REL_IM * l.re.abs();
    if !candidate {
        return HoleVerdict::NotCandidate;
    }
    let Some(lowest) = lowest_returned_bound else {
        // Empty-bound-set rule (issue #913).
        let resolved = c.residual <= EMPTY_SET_RESIDUAL_CAP;
        if !resolved {
            return HoleVerdict::Unresolved;
        }
        let physical_curl = c.curl_ratio > rule.empty_set_curl_floor;
        if !physical_curl {
            return HoleVerdict::BelowEmptySetCurl;
        }
        if let (Some((k0, eps_max, eps_min)), Some(eps_x)) = (rule.rayleigh, c.eps_weighted) {
            let violation = rayleigh_identity_violation_of(l.re, c.curl_ratio, eps_x, k0);
            if !rayleigh_consistent_of(violation, c.curl_ratio, eps_max, eps_min) {
                return HoleVerdict::RayleighInconsistent;
            }
        }
        return HoleVerdict::Hole;
    };
    if l.re - w > lowest {
        HoleVerdict::Hole
    } else {
        HoleVerdict::NotCandidate
    }
}

/// Text of the empty-bound-set warning (issue #913) for the reported pair
/// `c`: what was seen, why it may not be a guided mode, and how to decide.
///
/// The window fraction `(Re β² − lo)/(hi − lo)` is the pair's normalized
/// propagation constant `b` when the window is `(ε_clad k₀², ε_core k₀²)`.
fn empty_set_warning(
    solver: &str,
    c: &WithheldCandidate,
    guided_window: (f64, f64),
    rule: &HoleRule,
    lanczos_steps: usize,
) -> String {
    let (lo, hi) = guided_window;
    let b = (c.beta_sq.re - lo) / (hi - lo);
    format!(
        "{solver}: WARNING (issue #913, warn-only): no bound mode is returned, but the checked \
         Lanczos solve withheld a localized, bound-like Ritz pair in the guided window: β² = \
         {:.6e}{:+.3e}i (window fraction b ≈ {b:.3}), relative residual {:.3e} > \
         {DIELECTRIC_RESIDUAL_TOL:.0e} after {lanczos_steps} Lanczos steps, curl ratio {:.3e} > \
         empty-set threshold {:.3e}. The result is returned unchanged. This pair is NOT \
         confirmed as a guided mode: it may be a classifier artifact, a member of the low-curl \
         ladder of PML-box / cladding pairs (issue #947), as it is on the one fixture where \
         this warning has been measured (a thin core at V ≈ 0.43, analytic LP01 b = 4.7e-9, \
         reported pair b ≈ 0.42). To decide: (1) compare b with the analytic value for your V \
         number; (2) re-solve on a radially refined mesh and with a different outer (PML or \
         PEC box) radius: a guided mode keeps its β², a ladder pair drifts and tracks its core \
         energy fraction; (3) request more modes (the Lanczos budget scales with `n_modes`) so \
         the pair converges and its field can be inspected.",
        c.beta_sq.re, c.beta_sq.im, c.residual, c.curl_ratio, rule.empty_set_curl_floor,
    )
}

/// One evaluation of the empty-bound-set rule, recorded in test builds so
/// a test can assert what a classifier attempt reported (the warning is
/// otherwise only a log line): the solver and the reported pair, if any.
#[cfg(test)]
pub(crate) type EmptySetEvent = (&'static str, Option<WithheldCandidate>);

#[cfg(test)]
thread_local! {
    /// Empty-set rule evaluations made on this thread, in order. A test
    /// drains it with `take_empty_set_events`.
    pub(crate) static EMPTY_SET_EVENTS: std::cell::RefCell<Vec<EmptySetEvent>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Apply [`selection_hole`]. With a reference mode a hit becomes
/// [`EigenError::SelectionHole`] (issue #850). With none
/// (`lowest_returned_bound = None`, the empty-bound-set rule, issue #913) a
/// hit is logged as [`empty_set_warning`] and the result is `Ok`: the rule
/// is warn-only, so `EigenError::SelectionHole::lowest_returned` is always
/// the finite `β²` of a returned mode.
///
/// `rule` carries the curl floor the calling classifier applies to its
/// converged candidates (issue #916) and the empty-bound-set thresholds;
/// see [`selection_hole`].
fn check_selection_hole(
    solver: &'static str,
    localized_withheld: &[WithheldCandidate],
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
    lanczos_steps: usize,
) -> Result<(), EigenError> {
    let hole = selection_hole(
        localized_withheld,
        sigma,
        guided_window,
        lowest_returned_bound,
        rule,
    );
    if !localized_withheld.is_empty() {
        // Every localized withheld pair is listed; one a single test
        // excludes (the curl floor, or a test of the empty-set rule) is
        // labelled, so a skipped pair is visible, not silent (issues #916,
        // #913).
        let listed: Vec<String> = localized_withheld
            .iter()
            .map(|c| {
                format!(
                    "{:.6e}{:+.2e}i (ρ={:.2e}, curl={:.2e}{})",
                    c.beta_sq.re,
                    c.beta_sq.im,
                    c.residual,
                    c.curl_ratio,
                    hole_verdict(c, sigma, guided_window, lowest_returned_bound, rule).label(),
                )
            })
            .collect();
        let curl_floor = rule.curl_floor;
        let outcome = match (hole.is_some(), lowest_returned_bound.is_some()) {
            (true, true) => "HOLE",
            (true, false) => "EMPTY-SET WARNING (warn-only, no error)",
            (false, _) => "no hole",
        };
        eprintln!(
            "{solver}: hole check (issues #850, #916): {} localized withheld pair(s) [{}]; \
             curl floor {curl_floor:.2e}; reference (lowest returned / selected) bound β² = {}; \
             {outcome}",
            localized_withheld.len(),
            listed.join(", "),
            lowest_returned_bound.map_or_else(
                || format!(
                    "none (no bound mode returned: empty-set rule, issue #913, ρ ≤ \
                     {EMPTY_SET_RESIDUAL_CAP:.0e}, curl > {:.2e})",
                    rule.empty_set_curl_floor
                ),
                |b| format!("{b:.6e}")
            ),
        );
    }
    let Some(lowest_returned) = lowest_returned_bound else {
        // Empty-bound-set rule: warn, never fail (issue #913). The one
        // measured firing is a low-curl ladder pair (issue #947), not a
        // guided mode. Error-after-retry can return here once a committed
        // positive matches an analytic guided mode.
        #[cfg(test)]
        EMPTY_SET_EVENTS.with(|e| e.borrow_mut().push((solver, hole)));
        if let Some(c) = hole {
            eprintln!(
                "{}",
                empty_set_warning(solver, &c, guided_window, rule, lanczos_steps)
            );
        }
        return Ok(());
    };
    match hole {
        Some(c) => Err(EigenError::SelectionHole {
            solver,
            beta_sq_re: c.beta_sq.re,
            beta_sq_im: c.beta_sq.im,
            residual: c.residual,
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            lowest_returned,
            lanczos_steps,
        }),
        None => Ok(()),
    }
}

/// Real-path form of [`check_selection_hole`]: the localized withheld pairs
/// of a [`RawDielectricSolve`] are real, so trivially bound-like.
fn check_selection_hole_real(
    solver: &'static str,
    raw: &RawDielectricSolve,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> Result<(), EigenError> {
    check_selection_hole(
        solver,
        &raw.localized_withheld,
        raw.sigma,
        guided_window,
        lowest_returned_bound,
        rule,
        raw.lanczos_steps,
    )
}

/// What one classifier attempt produces: the selection it would return and
/// the outcome of its selection-level hole check (issue #850).
type ClassifiedAttempt<T> = (Vec<T>, Result<(), EigenError>);

/// Run a classifier attempt and, if its selection has a hole, retry **once**
/// with a doubled Lanczos request (issue #850).
///
/// Only the with-reference rule reaches this policy. An attempt that
/// returns no bound mode never reports a hole here: its empty-bound-set
/// rule is warn-only (issue #913; see [`selection_hole`]), so the attempt's
/// set is returned after one solve.
///
/// # Why retry, and why an error after that
///
/// A hole means a genuine eigenvalue that ranks inside the returned bound
/// set was still unconverged when the extension stopped. Measured on the
/// `formulation_audit_graddiv` ~3 %-step PEC fiber, the usual cause is a
/// **degenerate polarization twin**: one partner of the `β² = 35.385` pair
/// converged, the other was withheld (`ρ = 4.4×10⁻⁵`), and the 4-mode set
/// silently returned the next mode down in its place. Doubling the request
/// (which also doubles the extension cap) converges both partners. That is
/// a robust path, so a reasonable solve is not blocked: the hole is logged
/// and the retry's selection is returned. If the retry still has a hole
/// there is no further robust path inside the classifier, and the solve
/// fails with [`EigenError::SelectionHole`] rather than return a set with a
/// missing mode, which could be the true fundamental.
fn retry_on_selection_hole<T>(
    solver: &str,
    n_request: usize,
    mut attempt: impl FnMut(usize) -> Result<ClassifiedAttempt<T>, EigenError>,
) -> Result<Vec<T>, EigenError> {
    let (out, hole) = attempt(n_request)?;
    let Err(first) = hole else {
        return Ok(out);
    };
    let retry_request = 2 * n_request;
    eprintln!(
        "{solver}: WARNING selection hole at Lanczos request {n_request} ({first}); retrying          once with request {retry_request}"
    );
    let (out, hole) = attempt(retry_request)?;
    hole?;
    eprintln!("{solver}: retry at request {retry_request} closed the selection hole");
    Ok(out)
}

/// Complex (PML / DtN) counterpart of [`dielectric_checked_eigenpairs`]
/// (issue #834): shift-invert Lanczos on the complex-symmetric pencil
/// `A x = β² M₁ x` near the real shift `σ`, returning only **converged**
/// pairs.
///
/// The first pass is the historical `max_iters = n_req + 8` solve. When one
/// of its pairs with `Re β²` inside `guided_window` misses
/// [`DIELECTRIC_RESIDUAL_TOL`] but locates an eigenvalue, the same Krylov
/// run is extended, up to [`DIELECTRIC_LANCZOS_CAP_FACTOR`] times that
/// budget, until a converged pair confirms it (see
/// [`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked`]). A
/// pair that locates no eigenvalue (the bilinear Lanczos can return one
/// anywhere, issue #834) is withheld and never extends the run. When the
/// first pass converges, the result is bit-identical to the old unchecked
/// solve minus the withheld pairs.
///
/// # The hole check is at the selection (PR #847, issue #850)
///
/// Unlike [`crate::eigen::lossy_cavity`], this solve's contract is not "the
/// modes nearest `σ`": the guided window is densely populated with leaky
/// and PML-continuum pairs, some of which legitimately stay unconverged at
/// the cap, and the callers *select* from the converged set (bound before
/// leaky, then largest `Re β²`). A withheld localized pair nearer `σ` than
/// some returned continuum mode is therefore normal (the high-contrast
/// fiber withholds one at `β² = 34.94`, below its `35.8` fundamental), so
/// no solver-level hole check applies here. Each caller applies a hole rule
/// to its own selection: the analytic-cladding loop to its fundamental
/// ([`solve_dielectric_modes2_analytic_cladding_bc`]), and the classifiers
/// ([`solve_dielectric_modes2_pml`],
/// [`solve_dielectric_modes2_pml_profile_selected`]) to the bound modes
/// they return, via [`selection_hole`].
fn pml_checked_eigenpairs(
    a: SparseColMatRef<'_, usize, c64>,
    m1: SparseColMatRef<'_, usize, c64>,
    sigma: f64,
    n_request: usize,
    dim: usize,
    guided_window: (f64, f64),
) -> Result<CheckedComplexEigenpairs, EigenError> {
    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseComplexShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
    };
    solver.smallest_eigenpairs_checked(
        a,
        m1,
        n_req,
        ConvergenceCheck {
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            max_iters_cap: (max_iters * DIELECTRIC_LANCZOS_CAP_FACTOR).min(dim),
            window: Some(guided_window),
        },
    )
}

/// Log what a [`pml_checked_eigenpairs`] solve withheld (issue #834).
fn log_pml_withheld(who: &str, checked: &CheckedComplexEigenpairs, guided_window: (f64, f64)) {
    if checked.rejected.is_empty() && !checked.extended {
        return;
    }
    let in_window = checked
        .rejected
        .iter()
        .filter(|(l, _)| guided_window.0 < l.re && l.re < guided_window.1)
        .count();
    eprintln!(
        "{who}: {} converged pair(s); withheld {} unconverged Ritz pair(s) (residual > \
         {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps{}), {in_window} of them in the \
         guided window",
        checked.pairs.len(),
        checked.rejected.len(),
        checked.lanczos_steps,
        if checked.extended { ", extended" } else { "" },
    );
}

/// Curl-energy floor for the **PML** dielectric path (Epic #303 PML-B,
/// issue #332).
///
/// This is deliberately much smaller than the high-contrast cap of the
/// PEC-path [`physical_curl_floor_p2`] (`3e-2`, which that floor also
/// scales down with the index contrast since issue #791; `1e-6` here is
/// its lower clamp). The `3e-2` value was calibrated for a
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
/// above `physical_curl_floor_p2`. The core-energy fraction
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
/// Returns [`EigenError::SelectionHole`] if a localized, bound-like withheld
/// Ritz pair still sits above the lowest returned *bound* mode after one
/// automatic retry with a doubled Lanczos request (issue #850; see
/// `selection_hole`).
/// When the solve returns **no** bound mode (an empty set, or leaky modes only),
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
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
    // Request a generous batch: the in-window band is densely populated
    // (gradient nullspace + bound + leaky), and the genuine bound cluster
    // sits a little below the ceiling, so a small request can miss the
    // fundamental. 40 comfortably samples the whole guided window for the
    // SMF-28-scale meshes this targets.
    let n_request = (n_modes + 36).max(40);
    retry_on_selection_hole("solve_dielectric_modes2_pml", n_request, |n_request| {
        solve_dielectric_modes2_pml_attempt(
            mesh,
            eps_r,
            region_tags,
            interior_dof_mask,
            r_pml_inner,
            r_outer,
            sigma_0,
            k0,
            n_modes,
            n_request,
        )
    })
}

/// One [`solve_dielectric_modes2_pml`] solve + classification at a given
/// Lanczos request (see [`solve_dielectric_modes_attempt`]).
#[allow(clippy::too_many_arguments)]
fn solve_dielectric_modes2_pml_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricModePml>, EigenError> {
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

    let raw = dielectric_raw_candidates_p2_pml(
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
    // No early return on an empty converged set: the solve may still have
    // withheld a bound-like pair, which the empty-set rule below must see
    // to warn about it (issue #913).
    let cands = &raw.cands;
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_pml();
    let hole_rule = HoleRule::new(curl_floor, eps_max, eps_min, None);

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
    for c in cands {
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
    // Relative to `Re β²` in every mesh length unit (issue #828; the old
    // `max(|Re β²|, 1)` floor made it absolute for `β² < 1`, e.g. a guide
    // meshed in metres or nm). Every candidate here is in the guided
    // window, so `Re β² > 0`.
    let is_bound = |m: &DielectricModePml| -> bool {
        m.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs()
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
    // Selection-level hole check (issue #850): a localized, bound-like
    // withheld pair above the lowest returned *bound* mode would have
    // ranked inside the bound part of this set. With no bound mode in the
    // set (it is empty, or holds leaky modes only) the empty-set rule
    // applies, which only warns (issue #913). `n_modes = 0` asks for
    // nothing.
    let lowest_bound = guided
        .iter()
        .filter(|m| is_bound(m))
        .map(|m| m.beta_sq.re)
        .reduce(f64::min);
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole(
            "solve_dielectric_modes2_pml",
            &raw.localized_withheld,
            raw.sigma,
            (beta_sq_floor, beta_sq_ceiling),
            lowest_bound,
            &hole_rule,
            raw.lanczos_steps,
        )
    };
    Ok((guided, hole))
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

// ===========================================================================
// Epic #339 (#363): analytic-LP₀₁ radial-profile selector
// ===========================================================================
//
// The PML solver (`solve_dielectric_modes2_pml`) cleanly isolates a
// **genuinely bound, genuinely core-confined** fundamental, but on a
// weakly-guiding fiber its `largest-Re(β²)`-among-bound pick lands an
// **over-confined artifact** (b ≈ 0.75, ~78 % error vs the oracle) rather
// than the physical LP₀₁ (PR #359 / #336 honest negatives). The bound-mode
// gate and the scalar `core_energy_fraction` gate are both **scalar** — a
// single integrated number that the over-confined artifact passes just as
// well as the genuine LP₀₁ (both clear ≳0.8). The genuine LP₀₁ is
// distinguished not by *how much* energy is in the core but by its **radial
// shape**: azimuthal order m = 0, **no radial nodes**, and a specific
// `J₀(u·r/a)` core / `K₀(w·r/a)` cladding envelope. An over-confined
// artifact decays too fast (wrong effective `w`) and a cladding-tail mode
// peaks outward — both **decorrelate** from the analytic LP₀₁ radial
// template even though their scalar core fractions are similar. This is the
// richer discriminant the scalar gates lack.
//
// This block adds, additively / opt-in (the existing
// `solve_dielectric_modes2_pml` and all its callers are untouched):
//
//   1. [`Lp01RadialTemplate`] — the analytic LP₀₁ radial envelope from the
//      oracle's `(V, b)` → `(u, w)`. The oracle defines only the *shape* to
//      correlate against; `b` is still **measured** from the FEM `n_eff`,
//      never imposed.
//   2. [`dielectric_mode_radial_profile_pml`] — the azimuthally-averaged
//      radial profile `|E(r)|` of a recovered [`DielectricModePml`], plus an
//      azimuthal-order (m = 0) variance and a radial-node count — a
//      generalization of [`dielectric_mode_field_shape_pml`]'s
//      quadrature/DOF plumbing (same `TRI_QUAD_DEG4`, same
//      `tri_nedelec2_basis_values`).
//   3. [`lp01_template_correlation`] — normalized radial correlation of a
//      profile against a template.
//   4. [`solve_dielectric_modes2_pml_profile_selected`] — the opt-in
//      selector: among the SAME in-window genuinely-bound, curl-bearing
//      survivors the current code keeps (gates UNCHANGED), rank by template
//      correlation subject to the m = 0 / zero-radial-node structural check.

/// The analytic LP₀₁ radial envelope of a step-index fiber, parameterized by
/// the scalar oracle's normalized propagation constant `b` (Epic #339, #363).
///
/// With `u = V·√(1−b)` and `w = V·√b`, the weakly-guiding LP₀₁ transverse
/// field magnitude has the radial shape
///
/// ```text
///   T(r) = J₀(u·r/a)                        for r < a   (core),
///   T(r) = [J₀(u)/K₀(w)] · K₀(w·r/a)        for r ≥ a   (cladding),
/// ```
///
/// continuous at `r = a`. This is the **physical template** the selector
/// correlates the recovered FEM field against — the oracle supplies only the
/// *shape* `(u, w)`, not the answer: the selected mode's `b` is still measured
/// from its FEM `n_eff`, never imposed.
#[derive(Clone, Copy, Debug)]
pub struct Lp01RadialTemplate {
    /// Core radius `a` (same length units as the sample radii).
    pub core_radius: f64,
    /// Normalized frequency `V = k₀·a·√(n_core²−n_clad²)`.
    pub v: f64,
    /// Oracle normalized propagation constant `b ∈ (0, 1)`.
    pub b: f64,
    /// Transverse core parameter `u = V·√(1−b)`.
    pub u: f64,
    /// Transverse cladding (decay) parameter `w = V·√b`.
    pub w: f64,
    /// Cladding-branch matching coefficient `J₀(u)/K₀(w)` (continuity at `a`).
    pub clad_coeff: f64,
}

impl Lp01RadialTemplate {
    /// Build the analytic LP₀₁ template from the fiber geometry and the
    /// scalar oracle's `b` (e.g. `normalized_b(fiber_lp_neff(.., 0, 1), ..)`).
    ///
    /// # Panics
    ///
    /// Panics if `core_radius ≤ 0`, `v ≤ 0`, or `b ∉ (0, 1)`.
    pub fn from_oracle_b(core_radius: f64, v: f64, b: f64) -> Self {
        assert!(
            core_radius > 0.0,
            "core_radius must be > 0; got {core_radius}"
        );
        assert!(v > 0.0, "V must be > 0; got {v}");
        assert!(b > 0.0 && b < 1.0, "oracle b must be in (0, 1); got {b}");
        let u = v * (1.0 - b).sqrt();
        let w = v * b.sqrt();
        let k0w = crate::analytic::fiber::bessel_k0(w);
        // J₀(u)/K₀(w): continuity of the envelope at r = a. K₀ > 0 for w > 0.
        let clad_coeff = crate::analytic::fiber::bessel_j0(u) / k0w;
        Self {
            core_radius,
            v,
            b,
            u,
            w,
            clad_coeff,
        }
    }

    /// Evaluate the analytic envelope `T(r)` at radius `r ≥ 0`.
    pub fn eval(&self, r: f64) -> f64 {
        let a = self.core_radius;
        if r < a {
            crate::analytic::fiber::bessel_j0(self.u * r / a)
        } else {
            self.clad_coeff * crate::analytic::fiber::bessel_k0(self.w * r / a)
        }
    }
}

/// Azimuthally-averaged radial profile of a recovered [`DielectricModePml`],
/// with the structural diagnostics the analytic-LP₀₁ selector needs (Epic
/// #339, #363).
///
/// Built by reusing [`dielectric_mode_field_shape_pml`]'s quadrature/DOF
/// plumbing: the transverse field `|E|` is reconstructed per triangle from
/// the mode's p=2 DOFs (`tri_nedelec2_basis_values` + the global DOF map) at
/// the degree-4 quadrature points [`TRI_QUAD_DEG4`], then binned by radius
/// (centroid → quadrature-point radius) into `n_bins` annular bins out to
/// `r_max`, **azimuthally averaged** within each bin (the energy-weighted
/// mean `|E|`). PML-region triangles (`tag == REGION_PML`) are excluded so
/// the absorbing layer does not pollute the profile.
#[derive(Clone, Debug)]
pub struct ModeRadialProfile {
    /// Bin-center radii (length `n_bins`), uniformly spaced on `[0, r_max)`.
    pub r: Vec<f64>,
    /// Azimuthally-averaged `|E|(r)` per bin (length `n_bins`); zero in empty
    /// bins. This is the **unsigned** magnitude profile correlated against the
    /// (also unsigned-magnitude) analytic template.
    pub e_mag: Vec<f64>,
    /// Azimuthal-variation figure (m = 0 diagnostic): the energy-weighted mean
    /// over radius of the within-bin coefficient of variation of `|E|`
    /// (azimuthal std / azimuthal mean). A true m = 0 (azimuthally symmetric)
    /// mode → small; an m ≥ 1 mode oscillates azimuthally → large.
    pub azimuthal_variation: f64,
}

impl ModeRadialProfile {
    /// Count the **radial nodes** of the azimuthally-averaged magnitude
    /// profile: the number of **interior local minima** that descend
    /// appreciably below the surrounding maxima. The LP₀₁ fundamental is a
    /// single core-peaked, monotone-decaying lobe → **zero** radial nodes;
    /// each interior minimum (a "ring" structure, the field returning toward
    /// zero between lobes) is a higher radial order LP₀ₘ (m ≥ 2) or hybrid
    /// signature.
    ///
    /// Working on the **non-negative magnitude** profile (rather than a noisy
    /// signed radial-component proxy) makes this robust: a smooth fundamental
    /// has no interior dip, a ring/donut mode dips toward zero at the center
    /// or between lobes. Only dips that fall below `DIP_FRAC` of the bracketing
    /// peak count, so f64 ringing in the evanescent tail does not manufacture
    /// spurious nodes.
    pub fn radial_node_count(&self) -> usize {
        let peak = self
            .e_mag
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max)
            .max(1e-300);
        // A genuine radial node ("ring") is an interior local minimum where
        // the field dips below `DIP_FRAC·peak` and then RECOVERS to a new lobe
        // above `LOBE_FRAC·peak`. Both the dip depth and the recovery height
        // are measured against the GLOBAL peak (not a local bracket), so the
        // monotone-decaying evanescent tail of a smooth fundamental — which
        // has small wiggles but never recovers to a real lobe — registers
        // **zero** nodes.
        const DIP_FRAC: f64 = 0.3;
        const LOBE_FRAC: f64 = 0.4;
        // Restrict to the structurally-meaningful region: from the first bin to
        // the last bin still carrying ≥1% of the peak (ignore the noise tail).
        let last = self
            .e_mag
            .iter()
            .rposition(|&v| v >= 1e-2 * peak)
            .unwrap_or(0);
        if last < 2 {
            return 0;
        }
        let e = &self.e_mag[..=last];
        let mut nodes = 0usize;
        let mut i = 1usize;
        while i < last {
            // Local minimum at i, dipping below DIP_FRAC·peak?
            if e[i] <= e[i - 1] && e[i] <= e[i + 1] && e[i] < DIP_FRAC * peak {
                // A node only if a real outer lobe recovers after the dip.
                let right_max = e[i + 1..=last].iter().cloned().fold(0.0_f64, f64::max);
                if right_max > LOBE_FRAC * peak {
                    nodes += 1;
                }
            }
            i += 1;
        }
        nodes
    }

    /// `true` if the magnitude profile is **core-peaked**: its global maximum
    /// lies inside half a core radius (the LP₀₁ fundamental peaks at `r = 0`).
    /// A ring/donut mode peaks at `r ≈ a` or further out and fails this — the
    /// structural discriminant the scalar core-energy fraction misses.
    pub fn is_core_peaked(&self, core_radius: f64) -> bool {
        let arg = self
            .e_mag
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        // Peak radius must be inside half a core radius — a genuine LP₀₁ peaks
        // at the axis; a ring mode peaks near or beyond r = a.
        self.r
            .get(arg)
            .map(|&r| r < 0.5 * core_radius)
            .unwrap_or(false)
    }
}

/// Extract the azimuthally-averaged radial profile + structural diagnostics
/// of a recovered PML dielectric mode (Epic #339, #363).
///
/// `n_bins` annular bins span `[0, r_max)`; `r_max` should reach into the
/// cladding (e.g. a few core radii) but stop short of the PML so the absorbing
/// region is excluded (PML-tagged triangles are skipped regardless). Reuses
/// the exact field reconstruction of [`dielectric_mode_field_shape_pml`].
///
/// Within each radial bin the field is accumulated in azimuthal sectors so a
/// per-bin azimuthal mean / variance can be formed (the m = 0 diagnostic).
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()`, if `mode.e_edges` is not
/// the p=2 DOF length for `mesh`, or if `n_bins == 0` or `r_max ≤ 0`.
pub fn dielectric_mode_radial_profile_pml(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricModePml,
    n_bins: usize,
    r_max: f64,
) -> ModeRadialProfile {
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
    assert!(n_bins > 0, "n_bins must be > 0");
    assert!(r_max > 0.0, "r_max must be > 0; got {r_max}");

    let n_edges = mesh.edges().len();
    let tri_edges = mesh.tri_edges();
    let dr = r_max / n_bins as f64;

    // Azimuthal sectors per radial bin for the m = 0 (azimuthal-variation)
    // diagnostic: accumulate |E|·weight and weight into N_SECT angular bins,
    // then form the within-bin azimuthal coefficient of variation.
    const N_SECT: usize = 12;
    let mut sect_e = vec![0.0_f64; n_bins * N_SECT];
    let mut sect_w = vec![0.0_f64; n_bins * N_SECT];

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        if tag == REGION_PML {
            continue;
        }
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

        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            // Physical coordinates of this quadrature point.
            let x = lam[0] * coords[0][0] + lam[1] * coords[1][0] + lam[2] * coords[2][0];
            let y = lam[0] * coords[0][1] + lam[1] * coords[1][1] + lam[2] * coords[2][1];
            let rr = (x * x + y * y).sqrt();
            if rr >= r_max {
                continue;
            }
            let bin = ((rr / dr) as usize).min(n_bins - 1);

            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = c64::new(0.0, 0.0);
            let mut ey = c64::new(0.0, 0.0);
            for k in 0..8 {
                ex += coef[k] * c64::new(vals[k][0], 0.0);
                ey += coef[k] * c64::new(vals[k][1], 0.0);
            }
            let emag = (ex.norm() * ex.norm() + ey.norm() * ey.norm()).sqrt();

            // Azimuthal sector index.
            let theta = y.atan2(x); // (−π, π]
            let frac = (theta + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
            let sect = ((frac * N_SECT as f64) as usize).min(N_SECT - 1);
            sect_e[bin * N_SECT + sect] += w * emag;
            sect_w[bin * N_SECT + sect] += w;
        }
    }

    let mut r = vec![0.0_f64; n_bins];
    let mut e_mag = vec![0.0_f64; n_bins];
    // Azimuthal variation: energy-weighted mean over radius of the per-bin
    // azimuthal coefficient of variation of the sector means.
    let mut az_num = 0.0_f64;
    let mut az_den = 0.0_f64;
    for bin in 0..n_bins {
        r[bin] = (bin as f64 + 0.5) * dr;
        // Per-sector azimuthal means within this radial bin.
        let mut sect_means = [0.0_f64; N_SECT];
        let mut n_active = 0usize;
        let mut bin_e_acc = 0.0_f64;
        let mut bin_w_acc = 0.0_f64;
        for s in 0..N_SECT {
            let we = sect_w[bin * N_SECT + s];
            if we > 0.0 {
                let m = sect_e[bin * N_SECT + s] / we;
                sect_means[s] = m;
                n_active += 1;
                bin_e_acc += sect_e[bin * N_SECT + s];
                bin_w_acc += we;
            }
        }
        if bin_w_acc > 0.0 {
            // Azimuthal (radial-bin) mean |E|.
            e_mag[bin] = bin_e_acc / bin_w_acc;
        }
        // Coefficient of variation across active sectors (m = 0 figure).
        if n_active >= 2 && e_mag[bin] > 0.0 {
            let mean = sect_means
                .iter()
                .take(N_SECT)
                .filter(|&&v| v != 0.0)
                .sum::<f64>()
                / n_active as f64;
            if mean > 0.0 {
                let var = sect_means
                    .iter()
                    .filter(|&&v| v != 0.0)
                    .map(|&v| (v - mean) * (v - mean))
                    .sum::<f64>()
                    / n_active as f64;
                let cv = var.sqrt() / mean;
                // Weight by bin energy so the (low-energy, noisy) tail does
                // not dominate the m = 0 figure.
                let we = e_mag[bin] * e_mag[bin];
                az_num += we * cv;
                az_den += we;
            }
        }
    }
    let azimuthal_variation = if az_den > 0.0 { az_num / az_den } else { 0.0 };

    ModeRadialProfile {
        r,
        e_mag,
        azimuthal_variation,
    }
}

/// Normalized radial correlation `⟨|E_FEM(r)|, T(r)⟩ / (‖E_FEM‖·‖T‖)` of a
/// recovered mode profile against the analytic LP₀₁ template (Epic #339,
/// #363), evaluated on the profile's own radial bins.
///
/// Returns a value in `[−1, 1]`; the physical LP₀₁ correlates near `+1`, an
/// over-confined artifact (too-fast decay / wrong effective `w`) or a
/// cladding-tail mode (peaks outward) correlates lower even at a comparable
/// scalar core-energy fraction. Both vectors are mean-free? No — these are
/// non-negative magnitude profiles, so we use the **uncentered** cosine
/// similarity (the natural shape-overlap figure for non-negative envelopes).
pub fn lp01_template_correlation(
    profile: &ModeRadialProfile,
    template: &Lp01RadialTemplate,
) -> f64 {
    let mut dot = 0.0_f64;
    let mut nf = 0.0_f64;
    let mut nt = 0.0_f64;
    for (&ri, &fi) in profile.r.iter().zip(profile.e_mag.iter()) {
        let ti = template.eval(ri).abs();
        dot += fi * ti;
        nf += fi * fi;
        nt += ti * ti;
    }
    let denom = (nf.sqrt() * nt.sqrt()).max(1e-300);
    dot / denom
}

/// Profile-correlation score of a single recovered mode against the analytic
/// LP₀₁ template, bundling the structural diagnostics the selector gates on
/// (Epic #339, #363).
#[derive(Clone, Debug)]
pub struct Lp01ProfileScore {
    /// Normalized radial correlation with the analytic template (≈1 for the
    /// physical LP₀₁).
    pub correlation: f64,
    /// Azimuthal-variation figure (small ⇒ m = 0, azimuthally symmetric).
    pub azimuthal_variation: f64,
    /// Radial-node count (0 ⇒ fundamental LP₀₁; ≥1 ⇒ higher radial order).
    pub radial_nodes: usize,
    /// `true` if the magnitude profile is **core-peaked** (peaks on-axis, the
    /// LP₀₁ signature) rather than ring-shaped (peaks near/beyond `r = a`).
    pub core_peaked: bool,
    /// The scalar core-energy fraction (the UNCHANGED confinement gate), kept
    /// alongside so callers can apply both the scalar and the shape gate.
    pub core_energy_fraction: f64,
}

/// A recovered PML mode together with its analytic-LP₀₁ profile score (Epic
/// #339, #363) — the population the profile selector ranks.
#[derive(Clone, Debug)]
pub struct ScoredDielectricModePml {
    /// The recovered mode (same `DielectricModePml` the base solver returns).
    pub mode: DielectricModePml,
    /// Its analytic-LP₀₁ profile score + structural diagnostics.
    pub score: Lp01ProfileScore,
}

/// Opt-in **profile-selected** PML dielectric solve (Epic #339, #363):
/// recover the SAME in-window, genuinely-bound, curl-bearing candidates as
/// [`solve_dielectric_modes2_pml`] (gates UNCHANGED), score each against the
/// analytic LP₀₁ radial template [`Lp01RadialTemplate`], and return them
/// **sorted by descending template correlation** among the structurally-LP₀₁
/// (m = 0, zero-radial-node) survivors.
///
/// This is purely additive: [`solve_dielectric_modes2_pml`] and every existing
/// caller are untouched. The selector reuses the proven p=2 / PML / complex-
/// Lanczos stack bit-for-bit (`dielectric_raw_candidates_p2_pml`); it only
/// changes **selection/scoring** over the already-recovered Ritz vectors.
///
/// The first returned element is the profile-selected fundamental: the
/// in-window, genuinely-bound, core-confined, **physically-LP₀₁-structured**
/// (highest template correlation, m = 0, zero radial nodes) mode. Its `b` is
/// still measured from its FEM `n_eff`, never imposed — the template supplies
/// only the *shape* to rank against.
///
/// # Gates (all UNCHANGED from the base solver)
///
/// - in-window: `n_clad² k₀² < Re(β²) < n_core² k₀²`,
/// - curl-bearing: `curl_ratio > physical_curl_floor_pml()`,
/// - genuinely bound: `|Im(β²)|/Re(β²) ≤ 1e-8`.
///
/// Structural shape gates (NEW, for ranking only — they do **not** relax the
/// above): a candidate is "LP₀₁-structured" if its azimuthal variation is
/// below `az_var_max` and it has zero radial nodes. If NO survivor is
/// LP₀₁-structured, the full bound population is returned ranked by
/// correlation anyway (so callers can inspect the honest negative).
///
/// # Arguments
///
/// As [`solve_dielectric_modes2_pml`], plus:
/// - `template` — the analytic LP₀₁ envelope ([`Lp01RadialTemplate`]).
/// - `n_radial_bins` / `profile_r_max` — radial-profile resolution and outer
///   sampling radius (should reach into the cladding, short of the PML).
/// - `az_var_max` — azimuthal-variation threshold for the m = 0 structural
///   gate.
///
/// # Errors
///
/// Returns [`EigenError`] if the complex eigensolve fails. Returns an empty
/// `Vec` if no in-window bound candidate exists. Returns
/// [`EigenError::SelectionHole`] if a localized, bound-like withheld Ritz
/// pair more confined (higher `Re β²`) than the selected mode survives one
/// automatic retry with a doubled Lanczos request (issue #850). The check
/// is anchored at the pick, not at the bottom of the returned ladder.
/// When the solve scores **no** in-window bound candidate,
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_pml_profile_selected(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    template: &Lp01RadialTemplate,
    n_radial_bins: usize,
    profile_r_max: f64,
    az_var_max: f64,
) -> Result<Vec<ScoredDielectricModePml>, EigenError> {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    // Same generous batch the base solver requests.
    let n_request = 40usize;
    retry_on_selection_hole(
        "solve_dielectric_modes2_pml_profile_selected",
        n_request,
        |n_request| {
            solve_dielectric_modes2_pml_profile_selected_attempt(
                mesh,
                eps_r,
                region_tags,
                interior_dof_mask,
                r_pml_inner,
                r_outer,
                sigma_0,
                k0,
                template,
                (n_radial_bins, profile_r_max, az_var_max),
                n_request,
            )
        },
    )
}

/// One [`solve_dielectric_modes2_pml_profile_selected`] solve + scoring at
/// a given Lanczos request (see [`solve_dielectric_modes_attempt`]).
/// `profile` is `(n_radial_bins, profile_r_max, az_var_max)`.
#[allow(clippy::too_many_arguments)]
fn solve_dielectric_modes2_pml_profile_selected_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    template: &Lp01RadialTemplate,
    profile: (usize, f64, f64),
    n_request: usize,
) -> Result<ClassifiedAttempt<ScoredDielectricModePml>, EigenError> {
    let (n_radial_bins, profile_r_max, az_var_max) = profile;
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let raw = dielectric_raw_candidates_p2_pml(
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
    // No early return on an empty converged set: the solve may still have
    // withheld a bound-like pair, which the empty-set rule below must see
    // to warn about it (issue #913).
    let cands = &raw.cands;
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_pml();
    let hole_rule = HoleRule::new(curl_floor, eps_max, eps_min, None);

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    let mut scored: Vec<ScoredDielectricModePml> = Vec::new();
    for c in cands {
        let in_window = c.beta_sq.re > beta_sq_floor && c.beta_sq.re < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        // The bound-mode gate of `solve_dielectric_modes2_pml`, relative in
        // every mesh length unit (issue #828; `Re β² > 0` in the window).
        let is_bound = c.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * c.beta_sq.re.abs();
        if !(in_window && has_curl && is_bound) {
            continue;
        }
        let beta = principal_sqrt_c64(c.beta_sq);
        let n_eff = beta / c64::new(k0, 0.0);
        let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        let mode = DielectricModePml {
            n_eff,
            beta,
            beta_sq: c.beta_sq,
            guided: true,
            e_edges,
        };
        let shape = dielectric_mode_field_shape_pml(mesh, region_tags, &mode);
        let profile = dielectric_mode_radial_profile_pml(
            mesh,
            region_tags,
            &mode,
            n_radial_bins,
            profile_r_max,
        );
        let correlation = lp01_template_correlation(&profile, template);
        scored.push(ScoredDielectricModePml {
            mode,
            score: Lp01ProfileScore {
                correlation,
                azimuthal_variation: profile.azimuthal_variation,
                radial_nodes: profile.radial_node_count(),
                core_peaked: profile.is_core_peaked(template.core_radius),
                core_energy_fraction: shape.core_energy_fraction,
            },
        });
    }
    if scored.is_empty() {
        // No in-window bound candidate converged: the empty-set rule warns
        // if a qualifying pair was withheld, and never errors (issue #913).
        let hole = check_selection_hole(
            "solve_dielectric_modes2_pml_profile_selected",
            &raw.localized_withheld,
            raw.sigma,
            (beta_sq_floor, beta_sq_ceiling),
            None,
            &hole_rule,
            raw.lanczos_steps,
        );
        return Ok((Vec::new(), hole));
    }

    // Rank: LP₀₁-structured (m = 0 AND zero radial nodes) candidates first,
    // then by descending template correlation. Non-structured survivors keep
    // their (lower) correlation ordering at the back so callers can inspect
    // the honest-negative case where nothing is structurally LP₀₁.
    let is_structured = |s: &ScoredDielectricModePml| -> bool {
        s.score.azimuthal_variation < az_var_max && s.score.radial_nodes == 0 && s.score.core_peaked
    };
    scored.sort_by(|a, b| {
        let (sa, sb) = (is_structured(a), is_structured(b));
        match (sa, sb) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        b.score
            .correlation
            .partial_cmp(&a.score.correlation)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    eprintln!(
        "solve_dielectric_modes2_pml_profile_selected (σ₀={sigma_0:.3}): scored {} in-window \
         bound candidate(s); template (V={:.4}, b={:.4}, u={:.4}, w={:.4}); top: corr={:.4}, \
         az_var={:.3e}, radial_nodes={}, core_frac={:.3}, b_fem={:.4}",
        scored.len(),
        template.v,
        template.b,
        template.u,
        template.w,
        scored[0].score.correlation,
        scored[0].score.azimuthal_variation,
        scored[0].score.radial_nodes,
        scored[0].score.core_energy_fraction,
        (scored[0].mode.n_eff.re * scored[0].mode.n_eff.re - n_clad * n_clad)
            / (n_core * n_core - n_clad * n_clad),
    );

    // Selection-level hole check (issue #850), anchored at the **pick**.
    // This family returns the whole scored bound population (ranked by
    // profile, not truncated to `n_modes`) so callers can inspect the
    // ladder; its contract is the selected mode `scored[0]`. Anchoring at
    // the lowest scored `Re β²` instead would put the reference at the
    // bottom of the near-cladding bound ladder, which never converges to
    // completeness: on the SMF-28 fixture 22 localized bound-like pairs
    // stay withheld above it even at a doubled request, so that rule could
    // never pass on the fixture this selector exists for. A localized,
    // bound-like withheld pair *more confined* (higher `Re β²`) than the
    // pick is the direct competitor the ranking never saw (the LP₀₁-
    // structured pick is the core-confined top of the ladder), so it is the
    // hole that matters here.
    let lowest_bound = scored.first().map(|s| s.mode.beta_sq.re);
    let hole = check_selection_hole(
        "solve_dielectric_modes2_pml_profile_selected",
        &raw.localized_withheld,
        raw.sigma,
        (beta_sq_floor, beta_sq_ceiling),
        lowest_bound,
        &hole_rule,
        raw.lanczos_steps,
    );

    Ok((scored, hole))
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
/// `e_edges` p=2 DOFs via `tri_nedelec2_basis_values` and the global
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
) -> Result<RawDielectricSolve, EigenError> {
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
        return Ok(RawDielectricSolve {
            cands: Vec::new(),
            withheld: 0,
            withheld_in_window: 0,
            localized_withheld: Vec::new(),
            sigma: 0.0,
            lanczos_steps: 0,
        });
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
    // ⟨ε⟩_x = (xᵀ M_ε x)/(xᵀ M₁ x), the field-weighted permittivity.
    let eps_weighted = |x_interior: &[f64]| -> f64 {
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
        let xm1x = sparse_quadratic_form(m1_sparse.as_ref(), x_interior);
        xmx / xm1x.abs().max(1e-300)
    };

    // The guided β² window the classifier keeps (`n_clad²k₀²`, ceiling²k₀²):
    // only unconverged pairs inside it are worth extending the run for.
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_eff_ceiling = n_eff_target.unwrap_or(n_core).min(n_core);
    let guided_window = (eps_min * k0_sq, n_eff_ceiling * n_eff_ceiling * k0_sq);
    let checked = dielectric_checked_eigenpairs(
        a_sparse.as_ref(),
        m1_sparse.as_ref(),
        sigma,
        n_request,
        dim,
        guided_window,
    )?;
    Ok(raw_dielectric_solve(
        checked,
        sigma,
        guided_window,
        curl_ratio,
        eps_weighted,
    ))
}

/// Solve the **p=2** metallic curl-curl transverse modal eigenproblem on a
/// PEC cross-section — the order-2 analogue of
/// [`solve_waveguide_modes_with_opts`], used for the metallic-cutoff
/// regression and the manufactured order-of-convergence gate (Epic #318
/// 2.5C). Assembles with [`assemble_2d_nedelec2_with_epsilon`] (uniform
/// ε ≡ 1), PEC-reduces with the p=2 interior-DOF mask, estimates the shift
/// via `estimate_modal_shift`, and returns the lowest `n_modes` physical
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

    // Residual-checked passes (issue #798), as in
    // [`solve_waveguide_modes_with_opts`].
    let mut n_request = (n_modes + 8).min(dim);
    loop {
        let pass = metallic_checked_modes(
            k_sparse.as_ref(),
            m_sparse.as_ref(),
            sigma,
            threshold,
            n_request,
            n_modes,
        )?;
        let physical: Vec<f64> = pass.physical.iter().map(|p| p.lambda.max(0.0)).collect();
        if physical.len() == n_modes && !pass.unresolved_below {
            return Ok(physical);
        }
        if n_request >= dim {
            return Err(EigenError::FaerGevd(format!(
                "p=2 metallic modal solve: only recovered {} of {n_modes} converged physical \
                 modes (threshold {threshold:.3e}, σ = {sigma:.3e}; {} unconverged Ritz \
                 pair(s) withheld)",
                physical.len(),
                pass.withheld
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

// ===========================================================================
// Epic #339 (#446): analytic-cladding boundary-condition solver (mode-matching)
// ===========================================================================
//
// The PML path (`solve_dielectric_modes2_pml`) and its profile selector
// (`solve_dielectric_modes2_pml_profile_selected`) both **discretize** the
// infinite exterior (a cladding annulus + UPML) and let the eigensolver find
// modes there. On a weakly-guiding fiber that discretized cladding continuum
// spawns a dense ladder of genuinely-bound box/cladding-resonance eigenpairs
// straddling the razor-thin `(n_clad², n_core²)` guided window — the root
// cause of the ≤1 %-b miss proven by #336/#359/#365.
//
// This block adds, **additively / opt-in**, an alternative bounded-domain
// solver that attacks that root cause by *removing* the discretized cladding
// continuum. It meshes only the core + a thin cladding collar (no PML annulus)
// and imposes the exact analytic exterior decay `K_l(κ·r)` as a β-dependent
// **DtN / Robin boundary condition** on the truncation circle:
//
//   ∂_r E / E |_{r=r_bc} = κ · K_l'(κ·r_bc) / K_l(κ·r_bc),   κ = √(β² − n_clad²k₀²).
//
// The exterior then has no discretized free modes to pollute the spectrum, so
// the ladder cannot form. Because the BC depends on the unknown `β²` through
// `κ`, the eigensolve is wrapped in a small **self-consistent** (fixed-point)
// outer loop over `β²` — the same containment discipline as `self_consistent.rs`
// (solve at the current guess, update from the selected eigenvalue, repeat to
// contraction). The proven p=2 curl-curl `K` / ε-mass `M_ε` assembly, the
// `SparseComplexShiftInvertLanczos` eigensolve, PEC reduction, and the
// `m=0`/zero-radial-node/core-peaked confirmation diagnostics are all reused
// **bit-for-bit** — the only new physics is the boundary term added to `K`.

/// Radial logarithmic derivative of the analytic exterior decay `K_l(κ·r)`
/// evaluated at the truncation radius `r_bc` (Epic #339, #446):
///
/// ```text
///   γ(β²) = κ · K_l'(κ·r_bc) / K_l(κ·r_bc),   κ = √(β² − n_clad²·k₀²) > 0,
/// ```
///
/// with `K_l'(x) = −K_1(x)` for `l = 0` and `K_l'(x) = −½(K_{l−1}(x) +
/// K_{l+1}(x))` for `l ≥ 1` (the standard modified-Bessel derivative
/// identity). For a bound mode `β² > n_clad²·k₀²` so `κ` is real and positive,
/// and `γ < 0` (the exterior field decays outward). This is the coefficient of
/// the boundary term that replaces the discretized-exterior PML annulus: it
/// enters the curl-curl operator as `K_eff = K − γ · S_bc`, where `S_bc` is the
/// boundary tangential mass ([`assemble_boundary_tangential_mass`]).
///
/// Returns `None` if `beta_sq ≤ n_clad²·k₀²` (radiating / not bound — `κ`
/// imaginary), so the caller can reject an out-of-window iterate.
fn analytic_cladding_bc_gamma(
    l: usize,
    beta_sq: f64,
    n_clad: f64,
    k0: f64,
    r_bc: f64,
) -> Option<f64> {
    let kappa_sq = beta_sq - n_clad * n_clad * k0 * k0;
    if kappa_sq <= 0.0 {
        return None;
    }
    let kappa = kappa_sq.sqrt();
    let x = kappa * r_bc;
    if x <= 0.0 {
        return None;
    }
    let kl = crate::analytic::fiber::bessel_k(l, x);
    if !kl.is_finite() || kl.abs() < 1e-300 {
        return None;
    }
    // K_l'(x): -K_1 for l=0, else -(K_{l-1}+K_{l+1})/2.
    let kl_prime = if l == 0 {
        -crate::analytic::fiber::bessel_k1(x)
    } else {
        -0.5 * (crate::analytic::fiber::bessel_k(l - 1, x)
            + crate::analytic::fiber::bessel_k(l + 1, x))
    };
    Some(kappa * kl_prime / kl)
}

/// Assemble the interior-restricted **boundary tangential mass** matrix `S_bc`
/// on the truncation circle `r = r_bc` for the DtN / Robin analytic-cladding
/// boundary condition (Epic #339, #446).
///
/// `S_bc[i][j] = ∮_{r=r_bc} (N_i·t̂)(N_j·t̂) ds` is the line integral of the
/// tangential-field product of the p=2 Nédélec basis functions over every mesh
/// edge that lies on the truncation circle (both endpoints at `r_bc`). It is
/// the boundary analogue of the volume mass `M₁` and shares the *exact same*
/// DOF numbering / orientation-sign scatter (`tri_nedelec2_dofs`,
/// `TRI_NEDELEC2_DOF_FLIPS`) and the same interior (PEC) restriction, so the
/// returned matrix is compatible entry-for-entry with the assembled `K`/`M`.
///
/// The tangential trace of each basis function is evaluated with a 3-point
/// Gauss–Legendre rule along the physical edge (degree-5 exact — ample for the
/// quadratic p=2 tangential trace). Only boundary edges contribute; every
/// interior edge integrates to zero.
///
/// Returns a real `dim × dim` sparse matrix (`dim` = interior DOF count). It is
/// symmetric by construction. With no boundary edges on `r_bc` it is the zero
/// matrix.
fn assemble_boundary_tangential_mass(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    r_bc: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n_edges = mesh.edges().len();
    let (renumber, dim) = interior_renumber(interior_dof_mask);
    let tri_edges = mesh.tri_edges();
    let on_boundary = disk_boundary_nodes(mesh, r_bc);

    // 3-point Gauss–Legendre on [0, 1] (mapped from the standard [-1, 1] rule).
    // Nodes/weights on [-1,1]: ±√(3/5), 0 with weights 5/9, 8/9. Mapped to
    // [0,1]: t = (ξ+1)/2, w = w_ξ/2.
    const GL3: [(f64, f64); 3] = [
        (0.112_701_665_379_258_3, 0.277_777_777_777_777_8),
        (0.5, 0.444_444_444_444_444_4),
        (0.887_298_334_620_741_7, 0.277_777_777_777_777_8),
    ];

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();

    for (tri_index, (tri, row)) in mesh.tris.iter().zip(tri_edges.iter()).enumerate() {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);

        // Each of the triangle's three local edges: does it lie on r = r_bc?
        for (le, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let na = tri[la] as usize;
            let nb = tri[lb] as usize;
            if !(on_boundary[na] && on_boundary[nb]) {
                continue;
            }
            let pa = coords[la];
            let pb = coords[lb];
            let edge_vec = [pb[0] - pa[0], pb[1] - pa[1]];
            let edge_len = (edge_vec[0] * edge_vec[0] + edge_vec[1] * edge_vec[1]).sqrt();
            if edge_len < 1e-300 {
                continue;
            }
            // Unit tangent along the edge (parameterization direction la→lb).
            let t_hat = [edge_vec[0] / edge_len, edge_vec[1] / edge_len];

            // Barycentric coordinates along the edge: only λ_la, λ_lb vary
            // (linearly), the third is 0. At parameter s ∈ [0,1]: λ_la = 1−s,
            // λ_lb = s, λ_other = 0.
            let other = 3 - la - lb; // remaining local vertex index (0+1+2=3)
            for &(s, w) in GL3.iter() {
                let mut lam = [0.0_f64; 3];
                lam[la] = 1.0 - s;
                lam[lb] = s;
                lam[other] = 0.0;
                let basis = tri_nedelec2_basis_values(&coords, lam);
                // Tangential trace t̂·N_k of each of the 8 basis functions.
                let mut t_trace = [0.0_f64; 8];
                for (k, tk) in t_trace.iter_mut().enumerate() {
                    *tk = basis[k][0] * t_hat[0] + basis[k][1] * t_hat[1];
                }
                let ds = w * edge_len; // physical arc-length weight
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
                        let val = si * sj * t_trace[i] * t_trace[j] * ds;
                        if val != 0.0 {
                            trips.push(Triplet::new(ri, rj, val));
                        }
                    }
                }
            }
            let _ = le;
        }
    }

    triplets_to_sparse(dim, &trips)
}

/// Embed a real interior operator into a complex (`c64`) sparse matrix with
/// zero imaginary part, preserving the sparsity pattern (Epic #339, #446). The
/// analytic-cladding path assembles the proven **real** p=2 operators and adds
/// only a real boundary term, so the whole pencil is real; embedding in `c64`
/// lets it reuse the `SparseComplexShiftInvertLanczos` path unchanged (the
/// recovered `β²` then carries `Im ≈ 0`, exactly as the σ₀ = 0 PML reduction).
fn embed_real_sparse_c64(a: SparseColMatRef<'_, usize, f64>) -> SparseColMat<usize, c64> {
    let n = a.nrows();
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(cp[n]);
    for j in 0..a.ncols() {
        for kk in cp[j]..cp[j + 1] {
            trips.push(Triplet::new(ri[kk], j, c64::new(v[kk], 0.0)));
        }
    }
    SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips)
        .expect("real→c64 embed of an already-valid sparse operator cannot fail")
}

/// Outcome of the analytic-cladding self-consistent `β²` loop
/// ([`solve_dielectric_modes2_analytic_cladding_bc`], Epic #339 #446). Records
/// whether the fixed point over the β-dependent DtN boundary condition
/// contracted, plus the diagnostics an honest-science benchmark needs.
#[derive(Clone, Debug)]
pub struct AnalyticCladdingBcMode {
    /// Complex effective index `n_eff = √(β²)/k₀` of the selected core mode.
    pub n_eff: c64,
    /// Selected generalized-pencil eigenvalue `β²` at convergence.
    pub beta_sq: c64,
    /// `true` if the self-consistent `β²` fixed point **converged**
    /// (`|Δβ²|/β² < tol` within the iteration budget). `false` if it hit the
    /// iteration cap without contracting — the honest-negative signal that the
    /// DtN loop did not settle.
    pub converged: bool,
    /// Number of eigensolves performed by the outer loop (≥ 1).
    pub iterations: usize,
    /// Full-length **complex** transverse field over the p=2 DOFs (length
    /// [`n_dof_2d_nedelec2`]); PEC-eliminated DOFs carry exact zeros. Suitable
    /// for the `dielectric_mode_*_pml` diagnostics via [`Self::as_pml_mode`].
    pub e_edges: Vec<c64>,
}

impl AnalyticCladdingBcMode {
    /// View this mode as a [`DielectricModePml`] so the existing
    /// `dielectric_mode_field_shape_pml` / `dielectric_mode_radial_profile_pml`
    /// diagnostics apply **unchanged** (Epic #339 #446). `guided` is set
    /// `true`; `beta` is the principal square root of `beta_sq`.
    pub fn as_pml_mode(&self) -> DielectricModePml {
        DielectricModePml {
            n_eff: self.n_eff,
            beta: principal_sqrt_c64(self.beta_sq),
            beta_sq: self.beta_sq,
            guided: true,
            e_edges: self.e_edges.clone(),
        }
    }
}

/// Solve the weakly-guiding dielectric fundamental with an **analytic-cladding
/// DtN / Robin boundary condition** instead of a discretized+PML exterior
/// (Epic #339, #446) — the mode-matching / continuum-removal approach.
///
/// The mesh (`mesh`, `eps_r`, `interior_dof_mask`) is a plain core + thin
/// cladding-collar [`disk_tri_mesh`] truncated at `r_bc` (the outer radius,
/// which must carry a PEC ring in `interior_dof_mask`); there is **no** PML
/// annulus. The proven real p=2 operators `(K, M_ε, M₁)` come from
/// `assemble_2d_nedelec2_sparse_interior` unchanged. The infinite exterior is
/// represented exactly by the boundary term `−γ(β²)·S_bc` added to `K`, where
/// `γ` is the analytic radial log-derivative of `K_l(κ·r)`
/// (`analytic_cladding_bc_gamma`) and `S_bc` is the boundary tangential mass
/// (`assemble_boundary_tangential_mass`).
///
/// Because `γ` depends on the unknown `β²` through `κ = √(β²−n_clad²k₀²)`, the
/// eigensolve is wrapped in a **self-consistent fixed point**: solve at the
/// current `β²` guess, pick the in-window, curl-bearing, most-core-confined
/// eigenpair, update the guess from its eigenvalue, and repeat until
/// `|Δβ²|/β² < tol` (converged) or the iteration cap is hit (honest negative:
/// the DtN loop did not contract). The `SparseComplexShiftInvertLanczos` path,
/// PEC reduction, and the curl / in-window gates are reused unchanged.
///
/// # Arguments
///
/// - `mesh` / `eps_r` / `interior_dof_mask` — plain core+collar disk mesh, its
///   per-triangle `ε_r`, and the p=2 PEC interior mask (`disk_pec_interior_dofs2`
///   at `r_bc`).
/// - `r_bc` — truncation radius (the mesh outer radius); the DtN condition is
///   imposed on this circle.
/// - `k0` — free-space wavenumber (> 0).
/// - `l` — analytic azimuthal order of the target mode (0 for LP₀₁).
/// - `n_modes` — number of candidate eigenpairs to request per solve (the
///   fundamental is selected among them).
/// - `max_iter` / `tol` — self-consistent-loop budget and relative-`β²`
///   convergence tolerance.
///
/// # Returns
///
/// The selected [`AnalyticCladdingBcMode`] (with the `converged` flag), or an
/// empty `Vec` if no in-window curl-bearing mode is found at the seed. Errors
/// propagate from the eigensolve, and an iteration fails with
/// [`EigenError::FaerGevd`] when a *localized* withheld Ritz pair (a genuine
/// eigenvalue still unconverged at the Lanczos cap) lies in the window
/// nearer `σ` than the selected fundamental: it could be the true
/// fundamental (PR #847).
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_analytic_cladding_bc(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    r_bc: f64,
    k0: f64,
    l: usize,
    n_modes: usize,
    max_iter: usize,
    tol: f64,
) -> Result<Vec<AnalyticCladdingBcMode>, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    assert!(r_bc > 0.0, "r_bc must be positive; got {r_bc}");
    assert!(max_iter > 0, "max_iter must be positive");

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    let k0_sq = k0 * k0;
    let beta_sq_ceiling = n_core * n_core * k0_sq;
    let beta_sq_floor = n_clad * n_clad * k0_sq;

    // Proven real p=2 operators (K, M_ε, M₁) — assembled ONCE (β-independent),
    // then embedded in c64 so the complex Lanczos path applies unchanged.
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, eps_r, interior_dof_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_c = embed_real_sparse_c64(ops.k.as_ref());
    let m_eps_c = embed_real_sparse_c64(ops.m_eps.as_ref());
    let m1_c = embed_real_sparse_c64(ops.m1.as_ref());
    // Boundary tangential mass S_bc — also β-independent, assembled ONCE.
    let s_bc = assemble_boundary_tangential_mass(mesh, interior_dof_mask, r_bc)?;
    let s_bc_c = embed_real_sparse_c64(s_bc.as_ref());

    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_pml();
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // curl-energy ratio r = |xᴴ K x| / (k₀² |xᴴ M_ε x|).
    let curl_ratio = |x: &[c64]| -> f64 {
        let xkx = sparse_quadratic_form_c64_herm(k_c.as_ref(), x).norm();
        let xmx = sparse_quadratic_form_c64_herm(m_eps_c.as_ref(), x).norm();
        let denom = (k0_sq * xmx).max(1e-300);
        xkx / denom
    };

    let n_req = n_modes.max(1).min(dim);

    // Seed β² near the guided-band ceiling (bound modes sit just below n_core²).
    let mut beta_sq = beta_sq_ceiling * (1.0 - 1e-3);
    let mut selected: Option<(c64, Vec<c64>)> = None;
    let mut converged = false;
    let mut iterations = 0usize;

    for it in 1..=max_iter {
        iterations = it;
        // Boundary coefficient at the current β² guess. If the guess falls out
        // of the bound window (κ² ≤ 0), clamp it just inside so κ is real.
        let gamma = match analytic_cladding_bc_gamma(l, beta_sq, n_clad, k0, r_bc) {
            Some(g) => g,
            None => {
                let clamped =
                    beta_sq_floor * (1.0 + 1e-6) + 0.5 * (beta_sq_ceiling - beta_sq_floor);
                beta_sq = clamped;
                analytic_cladding_bc_gamma(l, beta_sq, n_clad, k0, r_bc)
                    .expect("mid-window β² always yields a real κ")
            }
        };

        // K_eff = K − γ·S_bc  (the DtN boundary term).  A = k₀²M_ε − K_eff.
        let k_eff = sparse_axpy_c64(k_c.as_ref(), s_bc_c.as_ref(), c64::new(-gamma, 0.0))?;
        let a_sparse = sparse_pencil_a_c64(k_eff.as_ref(), m_eps_c.as_ref(), k0_sq)?;

        // Shift just below the guided-band ceiling (guided β² sit near it).
        // Converged pairs only (issue #834).
        let sigma = beta_sq_ceiling * (1.0 - 1e-3);
        let checked = pml_checked_eigenpairs(
            a_sparse.as_ref(),
            m1_c.as_ref(),
            sigma,
            n_req,
            dim,
            (beta_sq_floor, beta_sq_ceiling),
        )?;
        log_pml_withheld(
            "solve_dielectric_modes2_analytic_cladding_bc",
            &checked,
            (beta_sq_floor, beta_sq_ceiling),
        );
        let pairs = &checked.pairs;

        // Select the in-window, curl-bearing eigenpair with the LARGEST Re(β²)
        // (most confined) — the fundamental. The single-mesh gates match the
        // PML path; the analytic BC has removed the exterior continuum, so this
        // is the genuine core mode rather than a top-of-ladder artifact.
        let mut best: Option<(c64, &Vec<c64>)> = None;
        for pair in pairs {
            let bsq = pair.lambda;
            let in_window = bsq.re > beta_sq_floor && bsq.re < beta_sq_ceiling;
            let has_curl = curl_ratio(&pair.vector) > curl_floor;
            if !(in_window && has_curl) {
                continue;
            }
            match &best {
                Some((prev, _)) if prev.re >= bsq.re => {}
                _ => best = Some((bsq, &pair.vector)),
            }
        }

        // Hole check at the selection (PR #847): the fundamental is the
        // in-window mode nearest σ (just under the ceiling). A localized
        // withheld pair (a genuine eigenvalue, unconverged at the cap)
        // nearer σ than the pick could be the true fundamental; fail rather
        // than iterate on the next mode down.
        if let Some((pick, _)) = best {
            let reach = (pick.re - sigma).hypot(pick.im);
            let in_window = |l: c64| l.re > beta_sq_floor && l.re < beta_sq_ceiling;
            if let Some((lambda, residual)) = checked.localized_hole(sigma, reach, in_window) {
                return Err(EigenError::FaerGevd(format!(
                    "analytic-cladding modal solve (iteration {it}): Ritz pair β² = \
                     {lambda:.6e} (relative residual {residual:.3e} > \
                     {DIELECTRIC_RESIDUAL_TOL:.0e}) locates an eigenvalue nearer σ = \
                     {sigma:.6e} than the selected fundamental β² = {pick:.6e} but did not \
                     converge in {} Lanczos steps; refusing to select past it",
                    checked.lanczos_steps
                )));
            }
        }
        let Some((new_beta_sq, vec)) = best else {
            // No in-window mode at this guess — stop (nothing to return).
            if selected.is_none() {
                return Ok(Vec::new());
            }
            break;
        };
        let vec = vec.clone();

        // Convergence on the relative β² step (relative in every mesh length
        // unit, issue #828; `beta_sq` is in the guided window, so `> 0`).
        let rel = (new_beta_sq.re - beta_sq).abs() / beta_sq.abs();
        selected = Some((new_beta_sq, vec));
        beta_sq = new_beta_sq.re;
        if rel < tol {
            converged = true;
            break;
        }
    }

    let Some((final_beta_sq, vec_int)) = selected else {
        return Ok(Vec::new());
    };
    let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
    for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
        e_edges[full_idx] = vec_int[interior_idx];
    }
    let n_eff = principal_sqrt_c64(final_beta_sq) / c64::new(k0, 0.0);
    eprintln!(
        "solve_dielectric_modes2_analytic_cladding_bc (l={l}): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, r_bc={r_bc:.4}; β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         self-consistent loop {} after {iterations} iter(s); β²={:.6e}, Re(n_eff)={:.6}",
        if converged {
            "CONVERGED"
        } else {
            "did NOT converge"
        },
        final_beta_sq.re,
        n_eff.re,
    );
    Ok(vec![AnalyticCladdingBcMode {
        n_eff,
        beta_sq: final_beta_sq,
        converged,
        iterations,
        e_edges,
    }])
}

/// Compute `C = A + scale·B` for two complex sparse operators sharing
/// compatible dimensions (Epic #339 #446). Used to add the DtN boundary term
/// `−γ·S_bc` to the curl-curl stiffness. Duplicate `(row, col)` entries are
/// summed by `try_new_from_triplets` exactly as the scatter-add does.
fn sparse_axpy_c64(
    a: SparseColMatRef<'_, usize, c64>,
    b: SparseColMatRef<'_, usize, c64>,
    scale: c64,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let n = a.nrows();
    let mut trips: Vec<Triplet<usize, usize, c64>> =
        Vec::with_capacity(a.col_ptr()[n] + b.col_ptr()[n]);
    let push = |trips: &mut Vec<Triplet<usize, usize, c64>>,
                m: SparseColMatRef<'_, usize, c64>,
                s: c64| {
        let cp = m.col_ptr();
        let ri = m.row_idx();
        let v = m.val();
        for j in 0..m.ncols() {
            for kk in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[kk], j, s * v[kk]));
            }
        }
    };
    push(&mut trips, a, c64::new(1.0, 0.0));
    push(&mut trips, b, scale);
    triplets_to_sparse_c64(n, &trips)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #831: `principal_sqrt_c64` recovers the propagation loss
    /// `Im n_eff` of a weakly leaky mode to a few ulps, at `Im β² / Re β² =
    /// 1e-6` and `1e-10`, in µm (`β² ≈ 21.8 µm⁻²`) and metre (`≈ 2.18e13
    /// m⁻²`) units. References: `mpmath` at 50 digits on the exact binary64
    /// `β²` (rounded to binary64). The old cancelling form was off by `1.3e-5` / `1.1e-4` at
    /// `1e-6` and returned `Im n_eff = 0` at `1e-10`.
    #[test]
    fn principal_sqrt_c64_keeps_low_loss_im_n_eff() {
        let k0 = 4.05_f64;
        // (Re β², Im/Re, mpmath Im n_eff, mpmath Re n_eff)
        let cases = [
            (21.8, 1e-6, 5.764255570334466e-07, 1.1528511140671813),
            (21.8, 1e-10, 5.764255570335187e-11, 1.1528511140670372),
            (2.18e13, 1e-6, 0.5764255570334466, 1152851.1140671815),
            (2.18e13, 1e-10, 5.7642555703351866e-05, 1152851.1140670374),
        ];
        for (re, ratio, im_ref, re_ref) in cases {
            let beta_sq = c64::new(re, re * ratio);
            let n_eff = principal_sqrt_c64(beta_sq) / c64::new(k0, 0.0);
            let rel_im = (n_eff.im - im_ref).abs() / im_ref;
            let rel_re = (n_eff.re - re_ref).abs() / re_ref;
            assert!(
                rel_im <= 4.0 * f64::EPSILON && rel_re <= 4.0 * f64::EPSILON,
                "β² = {beta_sq}: n_eff = {n_eff}, rel err Im {rel_im:.2e}, Re {rel_re:.2e}"
            );
            // The leaky sign is kept, and the lossless branch stays exact.
            let conj = principal_sqrt_c64(c64::new(re, -re * ratio));
            assert!(conj.im < 0.0 && conj.re == principal_sqrt_c64(beta_sq).re);
        }
        assert_eq!(
            principal_sqrt_c64(c64::new(21.8, 0.0)).im.to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(principal_sqrt_c64(c64::new(-4.0, 0.0)), c64::new(0.0, 2.0));
        assert_eq!(principal_sqrt_c64(c64::new(0.0, 0.0)), c64::new(0.0, 0.0));
    }

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
    /// 4. An all-zero vector is a benign `Ok(())` no-op (no sign to pin,
    ///    so no cross-mesh-unstable argmax to guard against — issue #349).
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
        gauge_fix_eigenvector(&mesh, &edges, &mut v_pos, 0).expect("positive proj is gaugable");
        assert_eq!(
            v_pos, v,
            "positive-projection vector must be left unchanged"
        );

        // Negated → flipped back to positive projection.
        let mut v_neg: Vec<f64> = v.iter().map(|x| -x).collect();
        gauge_fix_eigenvector(&mesh, &edges, &mut v_neg, 0).expect("negated proj is gaugable");
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

        // All-zero vector → benign no-op (no sign to pin), stays zero.
        let mut z = vec![0.0_f64; n];
        gauge_fix_eigenvector(&mesh, &edges, &mut z, 0).expect("all-zero vector is a no-op");
        assert!(z.iter().all(|&x| x == 0.0));
    }

    /// **Loud guard for ungaugable modes** (issue #349): a non-zero
    /// eigenvector that is orthogonal to *every* reference field in the
    /// fixed basis must produce [`EigenError::UngaugableMode`], NOT a
    /// silent fall-through to the cross-mesh-unstable argmax pin.
    ///
    /// We construct the orthogonal vector directly on a tiny synthetic
    /// edge set so the orthogonality is exact and analytic rather than
    /// mesh-dependent. Two horizontal edges of equal length, both sitting
    /// on the box's bottom rail (`sy = 0`), carry equal-and-opposite
    /// weights `[+1, −1]`. Then for that vector:
    ///
    /// - the y-directed `sin(mπsx)` references (1, 2) dot a horizontal
    ///   tangent to exactly 0 edge-by-edge;
    /// - the x-directed `sin(nπsy)` references (3, 4) carry `sin(nπ·0)=0`
    ///   at both midpoints, so they vanish edge-by-edge;
    /// - the uniform-x catch-all (5) gives `+|t| − |t| = 0` by the
    ///   equal-and-opposite cancellation (both edges same length);
    /// - the uniform-y catch-all (6) dots a horizontal tangent to 0.
    ///
    /// So every projection is exactly zero and the gauge must refuse to
    /// pin the sign, returning the loud error with the offending mode
    /// index. This is the test that proves the #300 silent-argmax hazard
    /// can no longer be hit in production.
    #[test]
    fn gauge_fix_eigenvector_loud_on_ungaugable_mode() {
        // Bounding box [0,1]×[0,1]: nodes 2,3 force the box height so the
        // two weighted edges (0→1 and 4→5) sit at sy = 0 within it.
        let mesh = TriMesh {
            nodes: vec![
                [0.0, 0.0], // 0  ┐ first horizontal edge (0→1)
                [0.4, 0.0], // 1  ┘
                [0.0, 1.0], // 2  box-height anchor (top-left)
                [1.0, 1.0], // 3  box-width / height anchor (top-right)
                [0.6, 0.0], // 4  ┐ second horizontal edge (4→5)
                [1.0, 0.0], // 5  ┘  same length 0.4 as the first
            ],
            tris: vec![],
        };
        // Hand-built edge table (a < b by convention). Only the two
        // horizontal bottom edges carry weight; the rest are zeros so they
        // do not contribute to any projection.
        let edges = vec![[0u32, 1], [2, 3], [4, 5]];
        // Equal-and-opposite weights on the two equal-length horizontal
        // edges; zero on the spacer edge.
        let mut v = vec![1.0_f64, 0.0, -1.0];

        let err = gauge_fix_eigenvector(&mesh, &edges, &mut v, 7)
            .expect_err("a mode orthogonal to every reference must be loudly ungaugable");
        match err {
            EigenError::UngaugableMode {
                mode,
                best_rel_proj,
            } => {
                assert_eq!(mode, 7, "error must carry the offending mode index");
                assert!(
                    best_rel_proj < 1e-6,
                    "best relative projection {best_rel_proj:.3e} must be below the \
                     gauge floor for a genuinely orthogonal mode"
                );
            }
            other => panic!("expected UngaugableMode, got {other:?}"),
        }

        // The vector must be left untouched on the loud path (no partial
        // mutation): the caller is responsible for handling the error.
        assert_eq!(
            v,
            vec![1.0, 0.0, -1.0],
            "ungaugable vector must not be mutated on the error path"
        );
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
                if yc < h / 2.0 { 1 } else { 0 }
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
                if (yc - 0.5 * h).abs() < 0.5 * d { 1 } else { 0 }
            })
            .collect();
        let eps_r =
            epsilon_r_from_region_tags(
                &region_tags,
                |tag| {
                    if tag == 1 { eps_core } else { eps_clad }
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

    /// **Contrast-scaled p=2 curl floor** (issue #791). The floor must sit
    /// well below the exact Rayleigh bound `r < (ε_max − ε_min)/ε_max` on the
    /// curl ratio of any in-window eigenpair (otherwise no guided mode of a
    /// weakly-guiding fiber can pass), keep the calibrated `3e-2` for
    /// Si/SiO₂, and stay above the gradient-nullspace noise for any input.
    #[test]
    fn p2_curl_floor_scales_with_index_contrast() {
        // SMF-28: exact bound = 7.8e-3, below the old fixed 3e-2 floor.
        let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
        let bound = (e_core - e_clad) / e_core;
        let floor = physical_curl_floor_p2(e_core, e_clad);
        assert!(bound < HIGH_CONTRAST_CURL_FLOOR, "SMF-28 bound {bound:.3e}");
        assert!(
            (floor - GUIDED_CURL_FLOOR_FRACTION * (e_core - e_clad) / e_clad).abs() < 1e-18,
            "SMF-28 floor {floor:.3e} must be the scaled contrast"
        );
        // The measured SMF-28 guided fundamental (r = 7.2e-4, issue #791)
        // passes with margin; the old fixed floor rejected it.
        assert!(7.2e-4 > 5.0 * floor && 7.2e-4 < HIGH_CONTRAST_CURL_FLOOR);

        // Si/SiO₂ (contrast ≈ 4.7): exactly the calibrated high-contrast
        // floor, so the high-contrast floor is unchanged.
        for (n_core, n_clad) in [(3.45_f64, 1.45_f64), (3.48, 1.444)] {
            let f = physical_curl_floor_p2(n_core * n_core, n_clad * n_clad);
            assert_eq!(f, HIGH_CONTRAST_CURL_FLOOR, "Si/SiO2 ({n_core}, {n_clad})");
        }

        // Uniform ε (no contrast): the nullspace floor, never zero.
        assert_eq!(physical_curl_floor_p2(2.0, 2.0), physical_curl_floor_pml());

        // Monotone in the contrast, and always below 0.04 × the exact bound
        // (the `/ε_min` scaling never pushes the floor near the bound).
        let mut prev = 0.0;
        for i in 1..=200 {
            let e_core = 2.0 + 0.1 * f64::from(i);
            let f = physical_curl_floor_p2(e_core, 2.0);
            let exact_bound = (e_core - 2.0) / e_core;
            assert!(f >= prev, "floor must not decrease with contrast");
            assert!(
                f <= 0.04 * exact_bound + 1e-15,
                "floor {f:.3e} above 0.04 × exact bound {exact_bound:.3e}"
            );
            prev = f;
        }
    }

    /// **Rayleigh-identity rejection** (issue #791). Every eigenpair of
    /// `A x = β² M₁ x` satisfies `r = 1 − n_eff²/⟨ε⟩_x`, so an in-window
    /// candidate with `r ≥ (ε_max − ε_min)/ε_max`, or whose `δ` exceeds the
    /// tolerance, is not an eigenpair and must be rejected. Injects the two
    /// unconverged Ritz pairs the pre-#791 selection returned as
    /// "fundamentals" (values measured at mesh (5,48)), next to the genuine
    /// converged fundamentals of the same solves.
    #[test]
    fn p2_rayleigh_identity_rejects_unconverged_pairs() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let cand = |n_eff: f64, r: f64, eps_w: f64| RawDielectricCandidate {
            beta_sq: n_eff * n_eff * k0 * k0,
            curl_ratio: r,
            eps_weighted: eps_w,
            residual: 0.0,
            vector: Vec::new(),
        };
        // SMF-28.
        let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
        let genuine = cand(1.448_196_80, 7.2353e-4, 2.098_792);
        assert!(rayleigh_identity_violation(&genuine, k0) < 1e-6);
        assert!(rayleigh_consistent(&genuine, k0, e_core, e_clad));
        let old_pick = cand(1.445_372_57, 3.3259e-2, 2.090_577);
        // r above the exact bound: needs ⟨ε⟩ = n²/(1−r) > ε_core.
        assert!(old_pick.curl_ratio >= (e_core - e_clad) / e_core);
        assert!(1.445_372_57_f64.powi(2) / (1.0 - old_pick.curl_ratio) > e_core);
        assert!(!rayleigh_consistent(&old_pick, k0, e_core, e_clad));
        // An unconverged tail pair below the bound, caught by δ alone
        // (measured δ = 1.1e-5 against the threshold 7.8e-6).
        let tail = cand(1.446_870_79, 8.3127e-5, 2.093_586);
        assert!(tail.curl_ratio < (e_core - e_clad) / e_core);
        assert!(!rayleigh_consistent(&tail, k0, e_core, e_clad));

        // ~3 %-step fiber.
        let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
        let genuine = cand(1.470_557_30, 4.9671e-3, 2.173_334);
        assert!(rayleigh_consistent(&genuine, k0, e_core, e_clad));
        let old_pick = cand(1.468_777_68, 3.8441e-1, 2.153_019);
        assert!(1.468_777_68_f64.powi(2) / (1.0 - old_pick.curl_ratio) > e_core);
        assert!(!rayleigh_consistent(&old_pick, k0, e_core, e_clad));
    }

    /// **Intermediate contrast (SiN/SiO₂) and a constructed unconverged
    /// vector** (issue #791). At contrast ≈ 0.9 the p=2 floor is ≈ 9e-3,
    /// below the 3e-2 cap. On a 2-D-confined SiN strip the PEC p=2 solve
    /// must return a fundamental just below the physical (vertical-slab)
    /// index ceiling, whose curl ratio (≈ 2.5e-2) the old fixed 3e-2 floor
    /// would have rejected, and which satisfies the Rayleigh identity. A
    /// Ritz-like vector built by mixing two converged eigenvectors while
    /// keeping one reported `β²` breaks the identity and must be rejected.
    #[test]
    fn p2_dielectric_solve_intermediate_contrast_sin_strip() {
        let n_core = 2.0_f64; // SiN
        let n_clad = 1.45_f64; // oxide
        let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (w, h) = (3.0_f64, 3.0_f64);
        let (w_core, d_core) = (1.2_f64, 0.6_f64);
        let (mesh, eps_r, _interior_p1) =
            strip_fixture((30, 30), (w, h), (w_core, d_core), (e_core, e_clad));
        let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);

        let floor = physical_curl_floor_p2(e_core, e_clad);
        assert!(
            floor > 5e-3 && floor < HIGH_CONTRAST_CURL_FLOOR,
            "SiN floor {floor:.3e} must be in the scaled (uncapped) regime"
        );

        let modes =
            solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1).expect("p=2 dielectric solve");
        assert!(!modes.is_empty(), "SiN slab must return its fundamental");
        let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
            .expect("a 2-D-confined strip yields a physical ceiling");
        let gap = ceiling - modes[0].n_eff;
        eprintln!(
            "SiN strip p=2: n_eff = {:.6}, physical ceiling = {ceiling:.6}, gap = {gap:.2e}, \
             floor = {floor:.3e}",
            modes[0].n_eff
        );
        // Measured n_eff 1.830185, 7.3e-3 below the ceiling 1.837505.
        assert!(
            gap > 0.0 && gap < 2e-2,
            "SiN fundamental {:.6} must sit just below the physical ceiling {ceiling:.6}",
            modes[0].n_eff
        );

        // Raw candidates: every returned (kept) pair obeys the identity; a
        // mixed vector with a stale β² does not.
        let cands = dielectric_raw_candidates_p2(&mesh, &eps_r, &dof_mask, k0, 16, Some(ceiling))
            .expect("raw p=2 candidates")
            .cands;
        let fund = cands
            .iter()
            .find(|c| (c.beta_sq - modes[0].beta_sq).abs() <= 1e-12 * modes[0].beta_sq)
            .expect("the returned fundamental is a raw candidate");
        assert!(
            rayleigh_identity_violation(fund, k0) < 1e-8,
            "converged fundamental δ = {:.3e}",
            rayleigh_identity_violation(fund, k0)
        );
        // The old fixed floor would have rejected this fundamental.
        assert!(fund.curl_ratio > floor && fund.curl_ratio < HIGH_CONTRAST_CURL_FLOOR);

        // Mix the fundamental with the lowest-β² converged candidate.
        let other = cands
            .iter()
            .filter(|c| rayleigh_identity_violation(c, k0) < 1e-8)
            .min_by(|a, b| a.beta_sq.partial_cmp(&b.beta_sq).unwrap())
            .expect("a second converged candidate");
        assert!(other.beta_sq < 0.99 * fund.beta_sq);
        let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &dof_mask)
            .expect("interior operators");
        let mixed: Vec<f64> = fund
            .vector
            .iter()
            .zip(&other.vector)
            .map(|(a, b)| a + b)
            .collect();
        let xkx = sparse_quadratic_form(ops.k.as_ref(), &mixed);
        let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &mixed);
        let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &mixed);
        let fake = RawDielectricCandidate {
            beta_sq: fund.beta_sq,
            curl_ratio: xkx.abs() / (k0 * k0 * xmx),
            eps_weighted: xmx / xm1x,
            residual: f64::NAN,
            vector: mixed,
        };
        let delta = rayleigh_identity_violation(&fake, k0);
        eprintln!("mixed (unconverged) vector: δ = {delta:.3e}");
        assert!(
            !rayleigh_consistent(&fake, k0, e_core, e_clad),
            "a mixed vector with a stale β² (δ = {delta:.3e}) must be rejected"
        );
    }

    /// The PEC p=2 / p=1 fiber fixture of the #449/#791 audit: SMF-28-like
    /// or ~3 %-step core of radius `a` (µm) in a `6a` PEC disk, mesh (5,48).
    /// Returns `(mesh, region_tags, eps_r)`.
    fn fiber_fixture(n_core: f64, n_clad: f64, a: f64) -> (TriMesh, Vec<i32>, Vec<f64>) {
        let (mesh, tags) = disk_tri_mesh(a, 6.0 * a, 5, 48);
        let eps_r = epsilon_r_from_region_tags(&tags, |t| {
            if t == 1 {
                n_core * n_core
            } else {
                n_clad * n_clad
            }
        });
        (mesh, tags, eps_r)
    }

    /// **Residual check catches the leaking SMF-28 tail pair** (issue #798).
    /// With the historical budget (`max_iters = n_request + 8`, 16 pairs) the
    /// SMF-28 p=2 window contains unconverged in-window pairs with true
    /// residuals far above `DIELECTRIC_RESIDUAL_TOL`. The #791 Rayleigh check
    /// cannot be relied on to catch them, because its violation `δ` is second
    /// order in the residual. Measured on macOS/aarch64: one pair at
    /// `n_eff ≈ 1.446871` (residual `3.5e-3`, `δ = 1.1e-5`) cleared the
    /// threshold (`7.8e-6`) by only 1.4×, and the smallest-`δ` pair (residual
    /// `3.4e-4`, `δ = 1.0e-7`) passes it outright. The test picks the pair by
    /// these properties, not by its Ritz value, which is not reproducible
    /// across platforms. The checked solve returns only converged pairs, so
    /// none of them can reach the filters, and every returned candidate
    /// satisfies the Rayleigh identity to round-off.
    #[test]
    fn residual_check_withholds_smf28_tail_pair() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
        let (mesh, _tags, eps_r) = fiber_fixture(1.4504, 1.4447, 4.1);
        let dof_mask = disk_pec_interior_dofs2(&mesh, 6.0 * 4.1);
        let ceiling = physical_index_ceiling(&mesh, &eps_r, k0);

        // Re-run the historical, unchecked first pass on the same pencil.
        let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &dof_mask).unwrap();
        let k0_sq = k0 * k0;
        let a = sparse_pencil_a(ops.k.as_ref(), ops.m_eps.as_ref(), k0_sq).unwrap();
        let n_core = e_core.sqrt();
        let target = match ceiling {
            Some(c) if c < n_core => c * c * k0_sq,
            _ => e_core * k0_sq,
        };
        let sigma = target * (1.0 - 1e-3);
        let old = SparseShiftInvertLanczos {
            sigma,
            max_iters: 24,
            tol: 1e-9,
            inner: crate::eigen::lanczos::InnerSolver::Direct,
            precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
        }
        .smallest_eigenpairs(a.as_ref(), ops.m1.as_ref(), 16)
        .unwrap();
        let spmv = |mat: SparseColMatRef<'_, usize, f64>, x: &[f64]| -> Vec<f64> {
            let mut y = vec![0.0; x.len()];
            for (j, &xj) in x.iter().enumerate() {
                let range = mat.col_ptr()[j]..mat.col_ptr()[j + 1];
                for (&i, &v) in mat.row_idx()[range.clone()].iter().zip(&mat.val()[range]) {
                    y[i] += v * xj;
                }
            }
            y
        };
        // (residual, δ) of one historical pair.
        let diagnose = |pair: &EigenPair| -> (f64, f64) {
            let kx = spmv(a.as_ref(), &pair.vector);
            let mx = spmv(ops.m1.as_ref(), &pair.vector);
            let r2: f64 = kx
                .iter()
                .zip(&mx)
                .map(|(k, m)| (k - pair.lambda * m).powi(2))
                .sum();
            let m2: f64 = mx.iter().map(|m| m * m).sum();
            let residual = r2.sqrt() / (pair.lambda * m2.sqrt());
            let xkx = sparse_quadratic_form(ops.k.as_ref(), &pair.vector);
            let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &pair.vector);
            let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &pair.vector);
            let cand = RawDielectricCandidate {
                beta_sq: pair.lambda,
                curl_ratio: xkx / (k0_sq * xmx),
                eps_weighted: xmx / xm1x,
                residual,
                vector: Vec::new(),
            };
            (residual, rayleigh_identity_violation(&cand, k0))
        };
        // The leaking tail pair: an unconverged pair inside the guided window
        // whose Rayleigh violation is the smallest, i.e. the one closest to
        // slipping past the #791 check. It is selected by these properties,
        // not by its Ritz value: an unconverged Ritz value (residual ~ 1e-3)
        // is not reproducible across platforms (measured n_eff ≈ 1.446871 on
        // macOS/aarch64; it differs on Linux x86_64 CI).
        let in_guided = |lambda: f64| e_clad * k0_sq < lambda && lambda < e_core * k0_sq;
        let unconverged: Vec<(f64, f64, f64)> = old
            .iter()
            .filter(|p| in_guided(p.lambda))
            .map(|p| {
                let (residual, delta) = diagnose(p);
                (p.lambda, residual, delta)
            })
            .filter(|&(_, residual, _)| residual > 1e4 * DIELECTRIC_RESIDUAL_TOL)
            .collect();
        let &(tail_lambda, residual, delta) = unconverged
            .iter()
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .expect("the historical window holds an unconverged in-window tail pair");
        eprintln!(
            "SMF-28 tail pair: n_eff = {:.8}, residual = {residual:.3e}, δ = {delta:.3e} \
             ({} unconverged in-window pairs)",
            tail_lambda.sqrt() / k0,
            unconverged.len()
        );
        // Measured (macOS): residual 3.4e-4, δ 1.0e-7, under the #791
        // threshold. δ is second order in the residual, so it stays small.
        assert!(delta < 1e-4, "tail δ = {delta:.3e}");

        // The checked solve: only converged pairs, the tail value is gone.
        let raw = dielectric_raw_candidates_p2(&mesh, &eps_r, &dof_mask, k0, 16, ceiling).unwrap();
        eprintln!(
            "SMF-28 checked: {} converged candidates, {} withheld ({} in window), {} steps",
            raw.cands.len(),
            raw.withheld,
            raw.withheld_in_window,
            raw.lanczos_steps
        );
        assert!(!raw.cands.is_empty());
        for c in &raw.cands {
            assert!(
                c.residual <= DIELECTRIC_RESIDUAL_TOL,
                "residual {:.3e}",
                c.residual
            );
            assert!(
                rayleigh_identity_violation(c, k0) < 1e-10,
                "converged candidate δ = {:.3e}",
                rayleigh_identity_violation(c, k0)
            );
            for &(lambda, _, _) in &unconverged {
                assert!(
                    (c.beta_sq - lambda).abs() > 1e-9 * lambda,
                    "an unconverged tail value must not be returned"
                );
            }
        }
        assert!(rayleigh_consistent(
            raw.cands
                .iter()
                .find(|c| c.beta_sq < ceiling.unwrap_or(n_core).powi(2) * k0_sq)
                .expect("an in-window converged candidate"),
            k0,
            e_core,
            e_clad
        ));
    }

    /// **Requesting k modes returns k converged modes, or an explicit
    /// shortfall** (issue #798), on SMF-28 and the SiN strip (p=2). Each
    /// returned mode is a converged raw candidate. When fewer than `k`
    /// converged guided candidates exist, the withheld in-window count is
    /// non-zero, so the shortfall is visible.
    #[test]
    fn p2_requesting_k_modes_returns_converged_modes() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let k = 4;
        let smf = {
            let (mesh, _tags, eps_r) = fiber_fixture(1.4504, 1.4447, 4.1);
            let mask = disk_pec_interior_dofs2(&mesh, 6.0 * 4.1);
            (mesh, eps_r, mask)
        };
        let sin = {
            let (mesh, eps_r, _m1) =
                strip_fixture((30, 30), (3.0, 3.0), (1.2, 0.6), (4.0, 1.45_f64 * 1.45));
            let mask = rect_pec_interior_dofs2(&mesh, 3.0, 3.0);
            (mesh, eps_r, mask)
        };
        for (label, (mesh, eps_r, mask)) in [("SMF-28", smf), ("SiN strip", sin)] {
            let modes = solve_dielectric_modes2(&mesh, &eps_r, &mask, k0, k).unwrap();
            let ceiling = physical_index_ceiling(&mesh, &eps_r, k0);
            let raw =
                dielectric_raw_candidates_p2(&mesh, &eps_r, &mask, k0, (k + 8).max(16), ceiling)
                    .unwrap();
            eprintln!(
                "{label}: {} of {k} modes, {} converged candidates, {} withheld \
                 ({} in window), {} Lanczos steps; n_eff = {:?}",
                modes.len(),
                raw.cands.len(),
                raw.withheld,
                raw.withheld_in_window,
                raw.lanczos_steps,
                modes.iter().map(|m| m.n_eff).collect::<Vec<_>>()
            );
            assert!(
                modes.len() == k || raw.withheld_in_window > 0,
                "{label}: {} < {k} modes with no reported shortfall",
                modes.len()
            );
            for m in &modes {
                let c = raw
                    .cands
                    .iter()
                    .find(|c| (c.beta_sq - m.beta_sq).abs() <= 1e-12 * m.beta_sq)
                    .expect("every returned mode is a raw candidate");
                assert!(
                    c.residual <= DIELECTRIC_RESIDUAL_TOL,
                    "{label}: returned mode n_eff = {:.6} has residual {:.3e}",
                    m.n_eff,
                    c.residual
                );
            }
        }
    }

    /// **p=1 curl floor scales with the index contrast** (issue #794): the
    /// same formula as p=2, so the weakly-guiding SMF-28 floor sits far below
    /// the exact in-window bound and the high-contrast Si/SiO₂ floor stays at
    /// exactly `3e-2`.
    #[test]
    fn p1_curl_floor_scales_with_index_contrast() {
        let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
        let floor = physical_curl_floor(e_core, e_clad);
        assert!(
            (floor - GUIDED_CURL_FLOOR_FRACTION * (e_core - e_clad) / e_clad).abs() < 1e-18,
            "SMF-28 p=1 floor {floor:.3e} must be the scaled contrast"
        );
        assert!(floor < 0.04 * (e_core - e_clad) / e_core);
        for (n_core, n_clad) in [(3.45_f64, 1.45_f64), (3.48, 1.444)] {
            assert_eq!(
                physical_curl_floor(n_core * n_core, n_clad * n_clad),
                HIGH_CONTRAST_CURL_FLOOR,
                "Si/SiO2 ({n_core}, {n_clad}) p=1 floor must be unchanged"
            );
        }
        assert_eq!(physical_curl_floor(2.0, 2.0), physical_curl_floor_pml());
        // p=1 and p=2 share the formula.
        for i in 1..=50 {
            let e = 2.0 + 0.2 * f64::from(i);
            assert_eq!(physical_curl_floor(e, 2.0), physical_curl_floor_p2(e, 2.0));
        }
    }

    /// **Weak-contrast p=1 solve finds the fundamental** (issue #794). On the
    /// SMF-28 PEC disk the guided curl ratios (`r < 7.8e-3`) all sit below the
    /// old fixed `3e-2` p=1 floor, so `solve_dielectric_modes` used to return
    /// no guided mode. With the contrast-scaled floor it returns a
    /// core-guided, in-window, converged fundamental. The core fraction comes
    /// from the exact identity `⟨ε⟩_x = ε_clad + f·Δε` for this two-material
    /// cross-section.
    #[test]
    fn p1_dielectric_solve_weak_contrast_smf28() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n_core, n_clad) = (1.4504_f64, 1.4447_f64);
        let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
        let (mesh, _tags, eps_r) = fiber_fixture(n_core, n_clad, 4.1);
        let (_edges, interior) = disk_pec_interior_edges(&mesh, 6.0 * 4.1);

        let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 1).expect("p=1 solve");
        let fund = modes
            .first()
            .expect("p=1 SMF-28 must return its fundamental");
        assert!(fund.n_eff > n_clad && fund.n_eff < n_core);

        let ops = assemble_2d_nedelec_sparse_interior(&mesh, &eps_r, &interior).unwrap();
        let x: Vec<f64> = fund
            .e_edges
            .iter()
            .zip(&interior)
            .filter(|(_, keep)| **keep)
            .map(|(v, _)| *v)
            .collect();
        let xkx = sparse_quadratic_form(ops.k.as_ref(), &x);
        let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &x);
        let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &x);
        let r = xkx / (k0 * k0 * xmx);
        let eps_x = xmx / xm1x;
        let core_fraction = (eps_x - e_clad) / (e_core - e_clad);
        let oracle = crate::analytic::fiber::fiber_lp_neff(n_core, n_clad, 4.1, k0, 0, 1)
            .expect("LP01 is always guided");
        eprintln!(
            "p=1 SMF-28: n_eff = {:.6} (LP01 {oracle:.6}), r = {r:.3e}, core fraction = \
             {core_fraction:.3}, floor = {:.3e}",
            fund.n_eff,
            physical_curl_floor(e_core, e_clad)
        );
        assert!(
            core_fraction > 0.6,
            "fundamental must be core-guided (core fraction {core_fraction:.3})"
        );
        // The old fixed floor would have rejected it.
        assert!(r < HIGH_CONTRAST_CURL_FLOOR && r > physical_curl_floor(e_core, e_clad));
        // A converged eigenpair: the Rayleigh identity holds to round-off.
        let delta = (r - (1.0 - fund.n_eff * fund.n_eff / eps_x)).abs();
        assert!(delta < 1e-10, "Rayleigh identity δ = {delta:.3e}");
    }

    /// **High-contrast p=1 solve is unchanged** (issues #794, #798): on the
    /// Si/SiO₂ slab the p=1 floor is still `3e-2`, and the fundamental is the
    /// historical value (`n_eff = 3.016965` on the (4,60) slab, the converged
    /// first-pass pair of the pre-#798 solve).
    #[test]
    fn p1_dielectric_solve_high_contrast_unchanged() {
        let (n_core, n_clad) = (3.45_f64, 1.45_f64);
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (mesh, eps_r, interior) =
            slab_fixture(4, 60, 0.2, 3.0, 0.3, n_core * n_core, n_clad * n_clad);
        assert_eq!(
            physical_curl_floor(n_core * n_core, n_clad * n_clad),
            HIGH_CONTRAST_CURL_FLOOR
        );
        let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).unwrap();
        eprintln!(
            "p=1 Si slab (4,60): n_eff = {:?}",
            modes.iter().map(|m| m.n_eff).collect::<Vec<_>>()
        );
        assert!((modes[0].n_eff - 3.016_965).abs() < 1e-6);
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
            .expect("dielectric core")
            .cands;
        assert!(!cands.is_empty(), "solver returned no eigenpairs");

        // Dominant curl-carrying mode = largest β² with non-negligible curl
        // energy (rejecting the gradient cluster at the ceiling).
        let floor = physical_curl_floor(eps, eps);
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

    /// **Port-mode solve returns converged modes only** (issue #798,
    /// cross-check from PR #809). On the `b/16` rectangular guide the
    /// historical `n_modes = 12` solve returned an unconverged tail: TE₃₁ off
    /// by `1.06e-8` relative, and the near-degenerate pairs at `k_c² ≈ 39.31`
    /// and `49.34` merged into single values, while `n_modes ≥ 14` was right
    /// to `1e-12`. The residual-checked solve must give the same 12 lowest
    /// modes whether asked for 12 or 20.
    #[test]
    fn port_mode_solve_tail_matches_larger_request() {
        let mesh = rect_tri_mesh(32, 16, 2.0, 1.0);
        let (edges, mask) = rect_pec_interior_edges(&mesh, 2.0, 1.0);
        let m12 = solve_waveguide_modes(&mesh, &edges, &mask, 12).expect("12-mode solve");
        let m20 = solve_waveguide_modes(&mesh, &edges, &mask, 20).expect("20-mode solve");
        assert_eq!(m12.len(), 12);
        for (i, (a, b)) in m12.iter().zip(&m20).enumerate() {
            let rel = (a.lambda - b.lambda).abs() / b.lambda;
            assert!(
                rel < 1e-11,
                "mode {i}: k_c² = {:.10} (n=12) vs {:.10} (n=20), rel {rel:.2e}",
                a.lambda,
                b.lambda
            );
        }
        // TE₃₁ and both members of the near-degenerate pairs.
        assert!((m12[6].lambda - 32.106_193_548_1).abs() < 1e-8);
        assert!((m12[8].lambda - m12[7].lambda) > 5e-4);
        assert!((m12[11].lambda - m12[10].lambda) > 1e-3);
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
                    if tag == 1 { eps_core } else { eps_clad }
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
            .expect("real raw candidates must succeed")
            .cands;
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

    // ----- Selection-level hole check (issue #850) ----------------------

    /// The localized withheld pairs measured on the default-tier
    /// high-contrast PML fixture (`high_contrast_fiber_benchmark`, mesh
    /// `(4, 48)`, `n_modes = 4`): `(Re β², Im β², ρ)`.
    const HC_PML_WITHHELD_850: [(f64, f64, f64); 7] = [
        (34.93929, 5.98e-16, 1.37e-7),
        (34.88800, -2.00e-8, 4.15e-4),
        (34.75005, -4.65e-13, 4.03e-8),
        (34.72720, -1.18e-12, 1.80e-6),
        (34.70756, -7.99e-11, 4.60e-5),
        (34.69061, 1.57e-8, 6.78e-4),
        (34.67594, 1.34e-6, 4.91e-3),
    ];

    /// A withheld candidate with an explicit curl ratio (issue #916).
    fn withheld_916(beta_sq: c64, residual: f64, curl_ratio: f64) -> WithheldCandidate {
        WithheldCandidate {
            beta_sq,
            residual,
            curl_ratio,
            eps_weighted: None,
        }
    }

    /// Hole rule of a classifier whose own curl floor is `curl_floor`, with
    /// the empty-set threshold at its margin over that floor and no
    /// Rayleigh test (issue #913). The #850 / #916 tests, which all pass a
    /// reference mode, depend only on `curl_floor`.
    fn rule_913(curl_floor: f64) -> HoleRule {
        HoleRule {
            curl_floor,
            empty_set_curl_floor: EMPTY_SET_CURL_MARGIN * curl_floor,
            rayleigh: None,
        }
    }

    /// Curl ratio well inside the physical band, for the #850 rule tests
    /// that exercise the other conditions. The recorded #850 pairs carry no
    /// curl ratio (the check had none then).
    const PHYSICAL_CURL_916: f64 = 1e-2;

    fn hc_pml_withheld_850() -> Vec<WithheldCandidate> {
        HC_PML_WITHHELD_850
            .iter()
            .map(|&(re, im, r)| withheld_916(c64::new(re, im), r, PHYSICAL_CURL_916))
            .collect()
    }

    /// Negative control (issue #850): the `β² = 34.94` pair the high-contrast
    /// fixture withholds is localized and bound-like, but it lies *below*
    /// the lowest returned bound mode (`35.4753`), so it is not a hole. The
    /// same pair *is* a hole when the returned set reaches below it.
    #[test]
    fn selection_hole_negative_control_34_94_is_not_a_hole() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let window = (1.4447_f64.powi(2) * k0 * k0, 1.4874_f64.powi(2) * k0 * k0);
        let sigma = window.1 * (1.0 - 1e-3);
        let withheld = hc_pml_withheld_850();
        let floor = rule_913(physical_curl_floor_pml());
        assert_eq!(
            selection_hole(&withheld, sigma, window, Some(35.475_30), &floor),
            None,
            "the 34.94 pair below the lowest returned bound mode must not trip"
        );
        // A returned set reaching down to 34.5 would skip it: hole, and the
        // highest qualifying pair (34.939) is the one reported. 34.888 has
        // |Im| / Re = 5.7e-10 (bound-like) but sits below 34.939.
        let hole = selection_hole(&withheld, sigma, window, Some(34.5), &floor)
            .expect("a localized bound-like pair above the reference is a hole");
        assert_eq!(hole.beta_sq.re, 34.93929);
        assert_eq!(hole.residual, 1.37e-7);
    }

    /// Each condition of the hole rule is necessary (issue #850).
    #[test]
    fn selection_hole_requires_window_bound_like_and_certain_margin() {
        let window = (34.0, 36.0);
        let sigma = 35.96;
        let lowest = Some(35.0);
        let hole = |l: c64, r: f64| {
            selection_hole(
                &[withheld_916(l, r, PHYSICAL_CURL_916)],
                sigma,
                window,
                lowest,
                &rule_913(physical_curl_floor_pml()),
            )
        };
        // Baseline: in window, bound-like, certainly above 35.0.
        assert!(hole(c64::new(35.5, 0.0), 1e-6).is_some());
        // Leaky (|Im| / Re = 1e-6 > 1e-8): not bound-like.
        assert!(hole(c64::new(35.5, 35.5e-6), 1e-6).is_none());
        // Outside the window.
        assert!(hole(c64::new(36.2, 0.0), 1e-9).is_none());
        // At the ceiling to rounding (the gradient cluster at ε_core k₀²):
        // its uncertainty straddles the ceiling.
        assert!(hole(c64::new(36.0 - 1e-14, 0.0), 6e-8).is_none());
        // Above the reference by less than its own uncertainty
        // (w = 1e-3 · 35.96 ≈ 0.036 > 0.01): possibly the same eigenvalue.
        assert!(hole(c64::new(35.01, 0.0), 1e-3).is_none());
        // Same pair with a tight residual is certainly above: hole.
        assert!(hole(c64::new(35.01, 0.0), 1e-6).is_some());
    }

    /// Issue #916: a localized withheld pair above the reference that
    /// would be a hole on every other count is **not** one when its Ritz
    /// vector is curl-free (curl ratio at or below the classifier's floor):
    /// the classifier would have dropped it as gradient nullspace, so the
    /// selection is complete. The same pair with curl above the floor is a
    /// hole. Checked against each classifier's own floor.
    #[test]
    fn selection_hole_skips_gradient_pairs_below_curl_floor() {
        let window = (34.0, 36.0);
        let sigma = 35.96;
        let lowest = Some(35.0);
        let pair = c64::new(35.5, 0.0);
        // (floor function, its value): the PEC p=1 / p=2 floors at the
        // ~3 %-step graddiv fiber contrast (1.4874² / 1.4447²) and the PML
        // floor.
        let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
        let floors = [
            physical_curl_floor(e_core, e_clad),
            physical_curl_floor_p2(e_core, e_clad),
            physical_curl_floor_pml(),
        ];
        for floor in floors {
            let hole = |curl: f64| {
                selection_hole(
                    &[withheld_916(pair, 1e-6, curl)],
                    sigma,
                    window,
                    lowest,
                    &rule_913(floor),
                )
            };
            // Curl-free gradient pair (f64 noise) and a pair exactly at the
            // floor: never a hole.
            assert_eq!(
                hole(1e-13),
                None,
                "gradient pair tripped (floor {floor:.2e})"
            );
            assert_eq!(hole(floor), None, "pair at the floor tripped ({floor:.2e})");
            // Just above the floor: a hole.
            let hit = hole(floor * 1.01).expect("curl-bearing pair above the floor is a hole");
            assert_eq!(hit.beta_sq, pair);
        }
        // The #850 graddiv twin: curl 2.9e-3 against its 6.0e-4 floor
        // (p=1 floor at that contrast) is still a hole.
        let graddiv_floor = physical_curl_floor(e_core, e_clad);
        assert!(
            (graddiv_floor - 6.0e-4).abs() < 0.05e-4,
            "graddiv floor {graddiv_floor:.3e}"
        );
        assert!(
            selection_hole(
                &[withheld_916(pair, 4.4e-5, 2.9e-3)],
                sigma,
                window,
                lowest,
                &rule_913(graddiv_floor)
            )
            .is_some()
        );
        // A gradient pair higher in the window does not mask the physical
        // hole below it: the reported hole is the curl-bearing pair.
        let both = [
            withheld_916(c64::new(35.8, 0.0), 1e-7, 1e-13),
            withheld_916(pair, 1e-6, 1e-2),
        ];
        let hit = selection_hole(&both, sigma, window, lowest, &rule_913(graddiv_floor))
            .expect("the curl-bearing pair is a hole");
        assert_eq!(hit.beta_sq, pair);
        // A gradient pair alone above the reference: no error from the
        // typed check either.
        assert!(
            check_selection_hole(
                "solve_dielectric_modes",
                &both[..1],
                sigma,
                window,
                lowest,
                &rule_913(graddiv_floor),
                144,
            )
            .is_ok()
        );
    }

    /// The hole is surfaced as the typed [`EigenError::SelectionHole`] naming
    /// the withheld `β²`, its residual and the reference mode (issue #850).
    #[test]
    fn check_selection_hole_reports_typed_error() {
        let err = check_selection_hole(
            "solve_dielectric_modes2_pml",
            &[withheld_916(c64::new(35.5, 1e-12), 3e-6, PHYSICAL_CURL_916)],
            35.96,
            (34.0, 36.0),
            Some(35.0),
            &rule_913(physical_curl_floor_pml()),
            144,
        )
        .expect_err("hole must be an error");
        match &err {
            EigenError::SelectionHole {
                solver,
                beta_sq_re,
                residual,
                lowest_returned,
                lanczos_steps,
                ..
            } => {
                assert_eq!(*solver, "solve_dielectric_modes2_pml");
                assert_eq!(*beta_sq_re, 35.5);
                assert_eq!(*residual, 3e-6);
                assert_eq!(*lowest_returned, 35.0);
                assert_eq!(*lanczos_steps, 144);
            }
            other => panic!("expected SelectionHole, got {other:?}"),
        }
        let msg = err.to_string();
        assert!(
            msg.contains("3.550000e1") && msg.contains("Remedy"),
            "{msg}"
        );
    }

    fn selection_hole_err_850() -> EigenError {
        EigenError::SelectionHole {
            solver: "test",
            beta_sq_re: 35.5,
            beta_sq_im: 0.0,
            residual: 1e-6,
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            lowest_returned: 35.0,
            lanczos_steps: 40,
        }
    }

    /// Retry policy (issue #850): a hole retries once at a doubled request
    /// and returns the retry's selection when that closes it; a hole that
    /// survives the retry is a loud [`EigenError::SelectionHole`]; a clean
    /// first attempt never retries.
    #[test]
    fn retry_on_selection_hole_policy() {
        let mut seen = Vec::new();
        let out = retry_on_selection_hole("t", 16, |n| {
            seen.push(n);
            Ok((vec![n], Ok(())))
        })
        .unwrap();
        assert_eq!((out, seen), (vec![16], vec![16]));

        let mut seen = Vec::new();
        let out = retry_on_selection_hole("t", 16, |n| {
            seen.push(n);
            let hole = if n == 16 {
                Err(selection_hole_err_850())
            } else {
                Ok(())
            };
            Ok((vec![n], hole))
        })
        .unwrap();
        assert_eq!((out, seen), (vec![32], vec![16, 32]));

        let mut seen = Vec::new();
        let err = retry_on_selection_hole("t", 40, |n| {
            seen.push(n);
            Ok((vec![n], Err(selection_hole_err_850())))
        })
        .unwrap_err();
        assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");
        assert_eq!(seen, vec![40, 80]);
    }

    /// Real p=2 family on a real pencil (issue #850): the ~3 %-step PEC fiber
    /// of `formulation_audit_graddiv` at the classifier's own request (16)
    /// converges one partner of the degenerate `β² = 35.385` pair and
    /// withholds the other, so the 4-mode set would return `35.298` in its
    /// place. The attempt flags the hole; with the budget held low on the
    /// retry too it fails loudly; the public solve's doubled-request retry
    /// closes it and returns both partners.
    #[test]
    fn real_p2_classifier_flags_degenerate_twin_hole() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
        let outer_r = 6.0 * a_um;
        let (mesh, tags) = disk_tri_mesh(a_um, outer_r, 5, 48);
        let eps = epsilon_r_from_region_tags(&tags, |t| {
            if t == REGION_CORE {
                n_core * n_core
            } else {
                n_clad * n_clad
            }
        });
        let interior = disk_pec_interior_dofs2(&mesh, outer_r);

        let (modes, hole) =
            solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 4, 16).unwrap();
        assert_eq!(modes.len(), 4);
        match hole {
            Err(EigenError::SelectionHole {
                beta_sq_re,
                lowest_returned,
                ..
            }) => {
                assert!((beta_sq_re - 35.385).abs() < 1e-2, "withheld {beta_sq_re}");
                assert!((lowest_returned - 35.298).abs() < 1e-2, "{lowest_returned}");
            }
            other => panic!("expected a selection hole at request 16, got {other:?}"),
        }

        // Issue #916: the withheld twin is a physical pair. Its Ritz vector
        // carries curl energy above the classifier's own floor (measured
        // 2.9e-3 against 6.0e-4), so the curl filter keeps it as a hole; it
        // also obeys the exact in-window bound r < (ε_max − ε_min)/ε_max.
        let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
        let floor = physical_curl_floor_p2(e_core, e_clad);
        let raw = dielectric_raw_candidates_p2(
            &mesh,
            &eps,
            &interior,
            k0,
            16,
            physical_index_ceiling(&mesh, &eps, k0),
        )
        .unwrap();
        let twin = raw
            .localized_withheld
            .iter()
            .find(|c| (c.beta_sq.re - 35.385).abs() < 1e-2)
            .expect("the withheld twin is a localized withheld candidate");
        eprintln!(
            "withheld twin: β² = {:.6}, ρ = {:.2e}, curl = {:.3e}, floor = {floor:.3e}",
            twin.beta_sq.re, twin.residual, twin.curl_ratio
        );
        assert!(
            twin.curl_ratio > 2.0 * floor && twin.curl_ratio < (e_core - e_clad) / e_core,
            "twin curl ratio {:.3e} vs floor {floor:.3e}",
            twin.curl_ratio
        );

        // Budget forced low on both attempts: loud failure.
        let err = retry_on_selection_hole("solve_dielectric_modes2", 16, |_| {
            solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 4, 16)
        })
        .unwrap_err();
        assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");

        // Public solve: the retry closes the hole; both partners returned.
        let modes = solve_dielectric_modes2(&mesh, &eps, &interior, k0, 4).unwrap();
        let b: Vec<f64> = modes.iter().map(|m| m.beta_sq).collect();
        assert_eq!(b.len(), 4, "{b:?}");
        assert!(
            b.iter().filter(|x| (*x - 35.385).abs() < 1e-2).count() == 2,
            "both 35.385 partners after the retry: {b:?}"
        );
    }

    /// PML family on a real pencil (issue #850): the coarse `(3, 32)`
    /// high-contrast PML fiber with `n_modes = 4` and the Lanczos request
    /// forced down to 16 withholds the bound `β² ≈ 35.42` pair above the
    /// lowest returned bound mode (`≈ 35.21`). The attempt flags it and,
    /// with the budget held low on the retry, the solve fails loudly.
    #[test]
    fn pml_classifier_flags_hole_at_forced_low_request() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
        let clad_r = 8.0 * a_um;
        let outer_r = 11.0 * a_um;
        let (mesh, tags) = disk_tri_mesh_pml(a_um, clad_r, outer_r, 3, 32);
        let eps = epsilon_r_from_region_tags(&tags, |t| {
            if t == REGION_CORE {
                n_core * n_core
            } else {
                n_clad * n_clad
            }
        });
        let interior = disk_pec_interior_dofs2(&mesh, outer_r);
        let attempt = |n_request: usize| {
            solve_dielectric_modes2_pml_attempt(
                &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, n_request,
            )
        };
        let (_, hole) = attempt(16).unwrap();
        match hole {
            Err(EigenError::SelectionHole {
                beta_sq_re,
                beta_sq_im,
                lowest_returned,
                ..
            }) => {
                assert!((beta_sq_re - 35.42).abs() < 1e-2, "withheld {beta_sq_re}");
                assert!(beta_sq_im.abs() <= DIELECTRIC_BOUND_REL_IM * beta_sq_re);
                assert!(lowest_returned < beta_sq_re, "{lowest_returned}");
            }
            other => panic!("expected a selection hole at request 16, got {other:?}"),
        }
        let err = retry_on_selection_hole("solve_dielectric_modes2_pml", 16, |_| attempt(16))
            .unwrap_err();
        assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");
    }

    // ----- Empty-bound-set hole rule (issue #913) -----------------------

    /// Issue #913: with no returned bound mode, a resolved, in-window,
    /// bound-like withheld pair with physical curl is reported (verdict
    /// `Hole`, which the empty-set rule turns into a warning), and each of
    /// the empty-set conditions is necessary. On `main` every one of these
    /// returned `None` (the rule could not fire without a reference).
    #[test]
    fn empty_bound_set_hole_requires_each_empty_set_condition() {
        let window = (34.0, 36.0);
        let sigma = 35.96;
        let k0 = 4.0;
        let (eps_max, eps_min) = (36.0 / (k0 * k0), 34.0 / (k0 * k0));
        // Classifier floor 1e-4, empty-set threshold 1e-3, Rayleigh test on.
        let rule = HoleRule {
            curl_floor: 1e-4,
            empty_set_curl_floor: 1e-3,
            rayleigh: Some((k0, eps_max, eps_min)),
        };
        // A pair obeying the Rayleigh identity r = 1 − β²/(k₀² ⟨ε⟩) exactly.
        let beta_sq = 35.5_f64;
        let consistent = |curl: f64| Some(beta_sq / (k0 * k0 * (1.0 - curl)));
        let pair = |im: f64, residual: f64, curl: f64, eps_weighted: Option<f64>| {
            [WithheldCandidate {
                beta_sq: c64::new(beta_sq, im),
                residual,
                curl_ratio: curl,
                eps_weighted,
            }]
        };
        let verdict = |p: &[WithheldCandidate; 1]| hole_verdict(&p[0], sigma, window, None, &rule);
        let hole = |p: &[WithheldCandidate; 1]| selection_hole(p, sigma, window, None, &rule);

        // Baseline: every condition met.
        let good = pair(0.0, 1e-6, 5e-3, consistent(5e-3));
        assert_eq!(verdict(&good), HoleVerdict::Hole);
        assert_eq!(hole(&good), Some(good[0]));
        // Exactly at the residual cap still counts; just above does not.
        assert_eq!(
            verdict(&pair(0.0, EMPTY_SET_RESIDUAL_CAP, 5e-3, consistent(5e-3))),
            HoleVerdict::Hole
        );
        let unresolved = pair(0.0, 1.01 * EMPTY_SET_RESIDUAL_CAP, 5e-3, consistent(5e-3));
        assert_eq!(verdict(&unresolved), HoleVerdict::Unresolved);
        assert_eq!(hole(&unresolved), None);
        // Above the classifier floor but at / below the empty-set threshold.
        for curl in [2e-4, 1e-3] {
            let low = pair(0.0, 1e-6, curl, consistent(curl));
            assert_eq!(verdict(&low), HoleVerdict::BelowEmptySetCurl, "{curl:e}");
            assert_eq!(hole(&low), None);
        }
        // At / below the classifier floor: the #916 label, as with a reference.
        assert_eq!(
            verdict(&pair(0.0, 1e-6, 1e-4, consistent(1e-4))),
            HoleVerdict::BelowCurlFloor
        );
        // Breaks the Rayleigh identity (⟨ε⟩ off by 1 %, δ ≈ 1e-2 against a
        // tolerance of 1e-3 · (ε_max − ε_min)/ε_max ≈ 5.6e-5).
        let off = pair(0.0, 1e-6, 5e-3, consistent(5e-3).map(|e| 1.01 * e));
        assert_eq!(verdict(&off), HoleVerdict::RayleighInconsistent);
        assert_eq!(hole(&off), None);
        // Curl at the exact in-window bound (ε_max − ε_min)/ε_max: no
        // in-window eigenpair reaches it.
        let bound = (eps_max - eps_min) / eps_max;
        assert_eq!(
            verdict(&pair(0.0, 1e-6, bound, consistent(bound))),
            HoleVerdict::RayleighInconsistent
        );
        // The PML path carries no ⟨ε⟩ and applies no Rayleigh test.
        assert_eq!(verdict(&pair(0.0, 1e-6, 5e-3, None)), HoleVerdict::Hole);
        // Leaky (|Im| / Re = 1e-6) or outside the certain window: never.
        assert_eq!(
            verdict(&pair(35.5e-6, 1e-6, 5e-3, None)),
            HoleVerdict::NotCandidate
        );
        let at_ceiling = [WithheldCandidate {
            beta_sq: c64::new(36.0 - 1e-14, 0.0),
            residual: 6e-8,
            curl_ratio: 5e-3,
            eps_weighted: None,
        }];
        assert_eq!(verdict(&at_ceiling), HoleVerdict::NotCandidate);
        // A NaN diagnostic is never a hole.
        assert_eq!(hole(&pair(0.0, f64::NAN, 5e-3, None)), None);
        assert_eq!(hole(&pair(0.0, 1e-6, f64::NAN, None)), None);

        // A window with no interior (uniform ε) admits no pair at all.
        let flat = (35.5, 35.5);
        assert_eq!(selection_hole(&good, sigma, flat, None, &rule), None);

        // The highest qualifying pair is the one reported.
        let two = [
            good[0],
            WithheldCandidate {
                beta_sq: c64::new(35.7, 0.0),
                residual: 1e-6,
                curl_ratio: 5e-3,
                eps_weighted: None,
            },
        ];
        assert_eq!(
            selection_hole(&two, sigma, window, None, &rule).map(|c| c.beta_sq.re),
            Some(35.7)
        );
    }

    /// Issue #913: the empty-set thresholds a classifier builds. The curl
    /// threshold is twice the contrast-scaled floor on every path, the PML
    /// path (own floor `1e-6`) included, and never below the classifier's
    /// own floor.
    #[test]
    fn hole_rule_empty_set_threshold_is_contrast_scaled_on_every_path() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        // ~3 %-step fiber: contrast floor 6.0e-4 → threshold 1.2e-3.
        let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
        let contrast_floor = physical_curl_floor(e_core, e_clad);
        let pec = HoleRule::new(contrast_floor, e_core, e_clad, Some(k0));
        assert_eq!(pec.curl_floor, contrast_floor);
        assert_eq!(pec.empty_set_curl_floor, 2.0 * contrast_floor);
        assert_eq!(pec.rayleigh, Some((k0, e_core, e_clad)));
        let pml = HoleRule::new(physical_curl_floor_pml(), e_core, e_clad, None);
        assert_eq!(pml.curl_floor, 1e-6);
        assert_eq!(pml.empty_set_curl_floor, 2.0 * contrast_floor);
        assert!((pml.empty_set_curl_floor - 1.2e-3).abs() < 1e-5);
        assert_eq!(pml.rayleigh, None);
        // SMF-28: 1.58e-4, 4.5× below the measured LP01 curl ratio 7.2e-4.
        let smf = HoleRule::new(1e-6, 1.4504_f64.powi(2), e_clad, None);
        assert!((smf.empty_set_curl_floor - 1.58e-4).abs() < 1e-6);
        assert!(7.2e-4 > 4.0 * smf.empty_set_curl_floor);
        // Si/SiO2: capped floor 3e-2 → 6e-2, below the genuine band 8.5e-2.
        let si = HoleRule::new(3e-2, 3.45_f64.powi(2), 1.45_f64.powi(2), Some(k0));
        assert_eq!(si.empty_set_curl_floor, 6e-2);
        // Never below the classifier's own floor.
        assert_eq!(
            HoleRule::new(0.5, e_core, e_clad, None).empty_set_curl_floor,
            0.5
        );
    }

    /// Drain the empty-set rule evaluations recorded on this thread.
    fn take_empty_set_events() -> Vec<EmptySetEvent> {
        EMPTY_SET_EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
    }

    /// Issue #913: the empty-set rule is warn-only. A reported pair is
    /// recorded and logged with actionable text, and the check returns
    /// `Ok`. No `SelectionHole` is built, so no `NaN` reference reaches a
    /// public error value. With a reference the same pair is still the
    /// typed #850 error.
    #[test]
    fn check_selection_hole_empty_set_warns_and_returns_ok() {
        let rule = rule_913(physical_curl_floor_pml());
        let window = (34.0, 36.0);
        let pair = [withheld_916(c64::new(35.5, 1e-12), 3e-6, PHYSICAL_CURL_916)];
        take_empty_set_events();
        let out = check_selection_hole(
            "solve_dielectric_modes2_pml",
            &pair,
            35.96,
            window,
            None,
            &rule,
            144,
        );
        assert!(out.is_ok(), "the empty-set rule never errors: {out:?}");
        assert_eq!(
            take_empty_set_events(),
            vec![("solve_dielectric_modes2_pml", Some(pair[0]))]
        );

        // The warning names the pair, says what it may be, and how to decide.
        let msg = empty_set_warning("solve_dielectric_modes2_pml", &pair[0], window, &rule, 144);
        for needle in [
            "solve_dielectric_modes2_pml: WARNING (issue #913, warn-only)",
            "no bound mode is returned",
            "β² = 3.550000e1",
            "window fraction b ≈ 0.750",
            "relative residual 3.000e-6",
            "after 144 Lanczos steps",
            "returned unchanged",
            "NOT confirmed as a guided mode",
            "issue #947",
            "radially refined mesh",
            "outer (PML or PEC box) radius",
            "request more modes",
        ] {
            assert!(msg.contains(needle), "missing {needle:?} in: {msg}");
        }
        assert!(!msg.contains("NaN"), "{msg}");

        // With a reference the rule is unchanged: the typed error, with a
        // finite reference, and no empty-set evaluation.
        let err = check_selection_hole(
            "solve_dielectric_modes2_pml",
            &pair,
            35.96,
            window,
            Some(35.0),
            &rule,
            144,
        )
        .unwrap_err();
        assert!(
            matches!(err, EigenError::SelectionHole { lowest_returned, .. } if lowest_returned == 35.0),
            "{err:?}"
        );
        let with_ref = err.to_string();
        assert!(
            with_ref.contains("above the lowest returned bound mode β² = 3.500000e1"),
            "{with_ref}"
        );
        assert!(take_empty_set_events().is_empty());

        // An unresolved pair (ρ above the cap) is evaluated and not reported.
        let unresolved = [withheld_916(c64::new(35.5, 1e-12), 3e-4, PHYSICAL_CURL_916)];
        assert!(check_selection_hole("t", &unresolved, 35.96, window, None, &rule, 144).is_ok());
        assert_eq!(take_empty_set_events(), vec![("t", None)]);
    }

    /// Issue #913: an empty-set report does not enter the retry policy.
    /// The attempt runs once and its (empty) set is returned, whether or
    /// not a pair was reported. A with-reference hole on the same pair
    /// still retries and then errors.
    #[test]
    fn empty_bound_set_warning_never_retries_or_errors() {
        let rule = rule_913(physical_curl_floor_pml());
        let window = (34.0, 36.0);
        let withheld = [withheld_916(c64::new(35.5, 0.0), 3e-6, PHYSICAL_CURL_916)];
        let check = |pairs: &[WithheldCandidate], lowest: Option<f64>| {
            check_selection_hole("t", pairs, 35.96, window, lowest, &rule, 24)
        };

        // Nothing returned, a qualifying pair withheld: one attempt, empty
        // set, one report.
        take_empty_set_events();
        let mut seen = Vec::new();
        let out = retry_on_selection_hole("t", 16, |n| {
            seen.push(n);
            Ok((Vec::<f64>::new(), check(&withheld, None)))
        })
        .unwrap();
        assert_eq!((out.len(), seen), (0, vec![16]));
        assert_eq!(take_empty_set_events(), vec![("t", Some(withheld[0]))]);

        // Only a gradient pair withheld: the same, with nothing reported.
        let gradient = [withheld_916(c64::new(35.5, 0.0), 3e-6, 1e-13)];
        let mut seen = Vec::new();
        let out = retry_on_selection_hole("t", 16, |n| {
            seen.push(n);
            Ok((Vec::<f64>::new(), check(&gradient, None)))
        })
        .unwrap();
        assert_eq!((out.len(), seen), (0, vec![16]));
        assert_eq!(take_empty_set_events(), vec![("t", None)]);

        // The same pair above a returned mode: retry, then the #850 error.
        let mut seen = Vec::new();
        let err = retry_on_selection_hole("t", 16, |n| {
            seen.push(n);
            Ok((vec![35.0_f64], check(&withheld, Some(35.0))))
        })
        .unwrap_err();
        assert_eq!(seen, vec![16, 32]);
        assert!(
            matches!(err, EigenError::SelectionHole { lowest_returned, .. } if lowest_returned == 35.0),
            "{err:?}"
        );
        assert!(take_empty_set_events().is_empty());
    }

    /// The ~3 %-step PEC fiber of `formulation_audit_graddiv` (mesh
    /// `(5, 48)`), shared by the real-path #913 tests.
    fn graddiv_fiber_913() -> (TriMesh, Vec<f64>, Vec<bool>, f64, (f64, f64)) {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
        let outer_r = 6.0 * a_um;
        let (mesh, tags) = disk_tri_mesh(a_um, outer_r, 5, 48);
        let eps = epsilon_r_from_region_tags(&tags, |t| {
            if t == REGION_CORE {
                n_core * n_core
            } else {
                n_clad * n_clad
            }
        });
        let interior = disk_pec_interior_dofs2(&mesh, outer_r);
        (mesh, eps, interior, k0, (n_core * n_core, n_clad * n_clad))
    }

    /// Issue #913 on a real pencil, real path: the localized withheld pairs
    /// of the request-16 graddiv solve are a low-curl ladder, not a
    /// gradient cluster. Evaluated with **no** reference mode (by hand:
    /// this solve does return bound modes, so its classifier never applies
    /// the empty-set rule), the rule
    /// reports the genuine `β² = 35.385` twin (curl `4.8×` the floor) and,
    /// once that is set aside, none of the ladder pairs: the `β² = 35.124`
    /// pair clears the classifier's floor by 1 % (so a plain
    /// "above the floor" empty-set rule would report it) and is excluded by
    /// the margin; the rest are at or below the floor.
    #[test]
    fn real_p2_low_curl_ladder_is_not_an_empty_set_hole() {
        let (mesh, eps, interior, k0, (e_core, e_clad)) = graddiv_fiber_913();
        let floor = physical_curl_floor_p2(e_core, e_clad);
        let rule = HoleRule::new(floor, e_core, e_clad, Some(k0));
        let ceiling = physical_index_ceiling(&mesh, &eps, k0);
        let raw = dielectric_raw_candidates_p2(&mesh, &eps, &interior, k0, 16, ceiling).unwrap();
        let n_ceiling = ceiling.unwrap_or(e_core.sqrt());
        let window = (e_clad * k0 * k0, n_ceiling * n_ceiling * k0 * k0);
        for c in &raw.localized_withheld {
            eprintln!(
                "withheld β² = {:.5}, ρ = {:.2e}, curl = {:.3e} ({:.3}× floor): {:?}",
                c.beta_sq.re,
                c.residual,
                c.curl_ratio,
                c.curl_ratio / floor,
                hole_verdict(c, raw.sigma, window, None, &rule)
            );
        }

        // The twin is the pair the empty-set rule reports.
        let hit = selection_hole(&raw.localized_withheld, raw.sigma, window, None, &rule)
            .expect("the withheld twin is reported with no reference mode");
        assert!((hit.beta_sq.re - 35.385).abs() < 1e-2, "{hit:?}");
        assert!(hit.residual <= EMPTY_SET_RESIDUAL_CAP && hit.curl_ratio > 4.0 * floor);

        // The ladder below it: no hole.
        let ladder: Vec<WithheldCandidate> = raw
            .localized_withheld
            .iter()
            .copied()
            .filter(|c| (c.beta_sq.re - 35.385).abs() >= 1e-2)
            .collect();
        assert!(ladder.len() >= 4, "{ladder:?}");
        assert_eq!(
            selection_hole(&ladder, raw.sigma, window, None, &rule),
            None
        );
        // The near-floor pair: resolved, in-window, above the classifier's
        // floor by about 1 %, and excluded only by the curl margin.
        let near = ladder
            .iter()
            .find(|c| (c.beta_sq.re - 35.124).abs() < 1e-2)
            .expect("the β² = 35.124 pair is withheld at request 16");
        assert!(
            near.curl_ratio > floor && near.curl_ratio < 1.1 * floor,
            "curl {:.4e} vs floor {floor:.4e}",
            near.curl_ratio
        );
        assert!(near.residual <= EMPTY_SET_RESIDUAL_CAP, "{near:?}");
        assert_eq!(
            hole_verdict(near, raw.sigma, window, None, &rule),
            HoleVerdict::BelowEmptySetCurl
        );
        // Every other ladder pair is at / below the classifier's floor.
        for c in ladder.iter().filter(|c| c.beta_sq.re != near.beta_sq.re) {
            assert_eq!(
                hole_verdict(c, raw.sigma, window, None, &rule),
                HoleVerdict::BelowCurlFloor,
                "{c:?}"
            );
        }
    }

    /// Issue #913: `n_modes = 0` asks for nothing, so the empty result is
    /// not an empty-set hole: the rule is not evaluated and nothing is
    /// warned, even on the graddiv solve whose request-16 pass withholds
    /// the genuine twin.
    #[test]
    fn zero_requested_modes_is_never_an_empty_set_hole() {
        let (mesh, eps, interior, k0, _) = graddiv_fiber_913();
        take_empty_set_events();
        let (modes, hole) =
            solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 0, 16).unwrap();
        assert!(modes.is_empty());
        assert!(hole.is_ok(), "{hole:?}");
        assert_eq!(take_empty_set_events(), Vec::new());
    }

    /// A PML disk fixture `(mesh, tags, eps, interior, clad_r, outer_r)`
    /// for the #913 PML tests.
    fn pml_disk_913(
        (n_core, n_clad): (f64, f64),
        core_r: f64,
        (clad_r, outer_r): (f64, f64),
        (n_rings, n_theta): (usize, usize),
    ) -> (TriMesh, Vec<i32>, Vec<f64>, Vec<bool>) {
        let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, n_rings, n_theta);
        let eps = epsilon_r_from_region_tags(&tags, |t| {
            if t == REGION_CORE {
                n_core * n_core
            } else {
                n_clad * n_clad
            }
        });
        let interior = disk_pec_interior_dofs2(&mesh, outer_r);
        (mesh, tags, eps, interior)
    }

    /// The thin-core fiber of the #913 PML tests: `a = 0.30 µm`,
    /// `n = 1.4874 / 1.4447`, PML box `(8, 11) µm`. `V ≈ 0.43`.
    const THIN_CORE_913: ((f64, f64), f64, (f64, f64)) = ((1.4874, 1.4447), 0.30, (8.0, 11.0));

    /// Issue #913 end to end on a real pencil, PML path, and the reason
    /// the empty-set rule only warns.
    ///
    /// On the thin-core fiber ([`THIN_CORE_913`], mesh `(4, 48)`) at a
    /// Lanczos request of 6 the classifier returns **no** bound mode while
    /// its solve withholds a resolved pair at `β² ≈ 35.155`
    /// (`ρ ≈ 8×10⁻⁸`). The empty-set rule reports that pair.
    ///
    /// The pair is **not** a withheld guided mode. This fiber has
    /// `V ≈ 0.43`, and its only guided mode, LP₀₁, has an analytic
    /// `b = 4.7×10⁻⁹` (checked below against [`fiber_lp_neff`]). The
    /// reported pair has `b ≈ 0.42`. The Judge's review of PR #948 measured
    /// that it drifts under radial refinement (`b = 0.417, 0.468, 0.529`)
    /// and with the PML box radius, and tracks its core energy fraction: it
    /// is a member of the low-curl ladder of issue #947, which the PML
    /// classifier returns as a "bound mode" once it converges (request 12
    /// below). The empty set at request 6 is the correct result.
    ///
    /// So the test pins warn-only: the pair is reported, the attempt and
    /// the retry policy return the empty set after one solve, and no error
    /// is raised. An error-after-retry policy would have replaced that
    /// empty set by the artifact.
    ///
    /// [`fiber_lp_neff`]: crate::analytic::fiber::fiber_lp_neff
    #[test]
    fn pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01() {
        use crate::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let ((n_core, n_clad), a, (clad_r, outer_r)) = THIN_CORE_913;

        // Analytic: V ≈ 0.43, LP01 (the only guided mode) at b ≈ 4.7e-9.
        let v = v_number(n_core, n_clad, a, k0);
        assert!((v - 0.4303).abs() < 1e-3, "V = {v}");
        let n_lp01 = fiber_lp_neff(n_core, n_clad, a, k0, 0, 1).expect("LP01 exists for all V");
        let b_lp01 = normalized_b(n_lp01, n_core, n_clad);
        assert!(
            b_lp01 > 0.0 && b_lp01 < 1e-7,
            "analytic LP01 b = {b_lp01:e}"
        );
        assert!(fiber_lp_neff(n_core, n_clad, a, k0, 0, 2).is_none());
        assert!(fiber_lp_neff(n_core, n_clad, a, k0, 1, 1).is_none());

        let (mesh, tags, eps, interior) =
            pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), (4, 48));
        let window = (n_clad * n_clad * k0 * k0, n_core * n_core * k0 * k0);
        let attempt = |n_request: usize| {
            solve_dielectric_modes2_pml_attempt(
                &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, n_request,
            )
        };
        let bound = |modes: &[DielectricModePml]| -> Vec<f64> {
            modes
                .iter()
                .filter(|m| m.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs())
                .map(|m| m.beta_sq.re)
                .collect()
        };

        // Request 6: no bound mode, no error, and one empty-set report.
        take_empty_set_events();
        let (modes, hole) = attempt(6).unwrap();
        assert!(bound(&modes).is_empty(), "request 6 returns no bound mode");
        assert!(hole.is_ok(), "the empty-set rule is warn-only: {hole:?}");
        let events = take_empty_set_events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].0, "solve_dielectric_modes2_pml");
        let pair = events[0]
            .1
            .expect("the request-6 solve reports a withheld pair");
        assert!((pair.beta_sq.re - 35.155).abs() < 2e-2, "{pair:?}");
        assert!(pair.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * pair.beta_sq.re);
        assert!(pair.residual <= EMPTY_SET_RESIDUAL_CAP, "{pair:?}");

        // The reported pair is nowhere near the analytic LP01: b ≈ 0.42
        // against 4.7e-9. It clears the curl threshold by about 1.33×.
        let b_pair = (pair.beta_sq.re - window.0) / (window.1 - window.0);
        assert!((b_pair - 0.417).abs() < 2e-2, "b = {b_pair}");
        assert!(b_pair > 1e6 * b_lp01, "b = {b_pair} vs LP01 {b_lp01:e}");
        let rule = HoleRule::new(
            physical_curl_floor_pml(),
            n_core * n_core,
            n_clad * n_clad,
            None,
        );
        let over = pair.curl_ratio / rule.empty_set_curl_floor;
        assert!(over > 1.0 && over < 1.7, "curl {over:.3}× the threshold");
        let msg = empty_set_warning("solve_dielectric_modes2_pml", &pair, window, &rule, 0);
        assert!(
            msg.contains("window fraction b ≈ 0.41") && msg.contains("issue #947"),
            "{msg}"
        );

        // The retry policy does not act on it: one solve, the empty set.
        let mut seen = Vec::new();
        let modes = retry_on_selection_hole("solve_dielectric_modes2_pml", 6, |n| {
            seen.push(n);
            attempt(n)
        })
        .unwrap();
        assert_eq!(seen, vec![6]);
        assert!(bound(&modes).is_empty());
        take_empty_set_events();

        // Issue #947, not this rule: at request 12 the same pair converges
        // and the PML classifier returns it as its top "bound mode".
        let (modes, hole) = attempt(12).unwrap();
        assert!(hole.is_ok(), "{hole:?}");
        let b = bound(&modes);
        assert!(
            (b[0] - pair.beta_sq.re).abs() < 1e-6,
            "the ladder pair {} is the top returned bound mode at request 12: {b:?}",
            pair.beta_sq.re
        );
        assert!(take_empty_set_events().is_empty());

        // The profile selector on the request-6 solve scores no bound
        // candidate. It reports the same pair and returns its empty ladder.
        let template = Lp01RadialTemplate::from_oracle_b(a, 0.43, 0.5);
        let (scored, hole) = solve_dielectric_modes2_pml_profile_selected_attempt(
            &mesh,
            &eps,
            &tags,
            &interior,
            clad_r,
            outer_r,
            6.0,
            k0,
            &template,
            (24, 4.0, 0.2),
            6,
        )
        .unwrap();
        assert!(scored.is_empty());
        assert!(hole.is_ok(), "{hole:?}");
        let events = take_empty_set_events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].0, "solve_dielectric_modes2_pml_profile_selected");
        let selector_pair = events[0].1.expect("the selector reports the same pair");
        assert!((selector_pair.beta_sq.re - pair.beta_sq.re).abs() < 1e-6);
    }

    /// Issue #913 on a real pencil that guides nothing, PML path: a weak
    /// **anti-guide** (`n_core = 1.4447 < n_clad = 1.4504`, SMF-28 contrast
    /// reversed) on mesh `(5, 48)` at a Lanczos request of 16. The
    /// classifier returns no bound mode and its solve withholds a ladder of
    /// **resolved**, in-window, bound-like PML-box pairs whose curl ratio
    /// (`≈ 6×10⁻⁶`) is several times the PML classifier's own floor. A
    /// rule that used that floor would report them. The contrast-scaled
    /// threshold (`1.58×10⁻⁴`) does not: the attempt evaluates the
    /// empty-set rule and reports nothing.
    ///
    /// (At its production request of 40 this classifier converges those
    /// ladder pairs and returns them as bound modes of the anti-guide. That
    /// is the classifier's own floor at work, the same on `main`, and not
    /// what this test pins.)
    #[test]
    fn pml_antiguide_low_curl_ladder_is_not_an_empty_set_hole() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n_core, n_clad, a) = (1.4447_f64, 1.4504_f64, 4.1_f64);
        let (clad_r, outer_r) = (8.0 * a, 11.0 * a);
        let (mesh, tags, eps, interior) =
            pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), (5, 48));
        let (e_max, e_min) = (n_clad * n_clad, n_core * n_core);
        let window = (e_min * k0 * k0, e_max * k0 * k0);

        // The withheld pairs of the request-16 solve.
        let raw = dielectric_raw_candidates_p2_pml(
            &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 16, None,
        )
        .unwrap();
        let pml_floor = physical_curl_floor_pml();
        let rule = HoleRule::new(pml_floor, e_max, e_min, None);
        assert!((rule.empty_set_curl_floor - 1.58e-4).abs() < 1e-6);
        // Pairs that pass every test but the empty-set curl threshold.
        let ladder: Vec<&WithheldCandidate> = raw
            .localized_withheld
            .iter()
            .filter(|c| {
                hole_verdict(c, raw.sigma, window, None, &rule) == HoleVerdict::BelowEmptySetCurl
            })
            .collect();
        assert!(
            ladder.len() >= 3,
            "expected a resolved low-curl bound-like ladder, got {} pair(s)",
            ladder.len()
        );
        for c in &ladder {
            assert!(c.residual <= EMPTY_SET_RESIDUAL_CAP, "{c:?}");
            assert!(
                c.curl_ratio > 2.0 * pml_floor && c.curl_ratio < 0.2 * rule.empty_set_curl_floor,
                "curl {:.3e}",
                c.curl_ratio
            );
        }
        assert_eq!(
            selection_hole(&raw.localized_withheld, raw.sigma, window, None, &rule),
            None
        );
        // The same pairs under the classifier's own floor: a hole.
        let naive = HoleRule {
            empty_set_curl_floor: pml_floor,
            ..rule
        };
        assert!(
            selection_hole(&raw.localized_withheld, raw.sigma, window, None, &naive).is_some(),
            "the PML floor alone would report the ladder"
        );

        // The classifier attempt at that request: no bound mode, and the
        // empty-set rule is evaluated once and reports nothing.
        take_empty_set_events();
        let (modes, hole) = solve_dielectric_modes2_pml_attempt(
            &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, 16,
        )
        .unwrap();
        assert!(
            modes
                .iter()
                .all(|m| m.beta_sq.im.abs() > DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs()),
            "request 16 returns no bound mode"
        );
        assert!(hole.is_ok(), "{hole:?}");
        assert_eq!(
            take_empty_set_events(),
            vec![("solve_dielectric_modes2_pml", None)]
        );
    }

    /// Issue #913 acceptance: a cross-section with **no** guided mode still
    /// returns an empty `Vec`, no error and no warning from every
    /// classifier. A uniform `ε` has an empty guided window, so no withheld
    /// pair can lie inside it (the request-16 solves below do withhold
    /// localized pairs).
    #[test]
    fn mode_free_uniform_cross_section_returns_empty_without_error() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let (n, a) = (1.4447_f64, 1.40_f64);

        let outer_r = 6.0 * a;
        let (mesh, tags) = disk_tri_mesh(a, outer_r, 5, 48);
        let eps = vec![n * n; tags.len()];
        let (_, interior1) = disk_pec_interior_edges(&mesh, outer_r);
        let interior2 = disk_pec_interior_dofs2(&mesh, outer_r);
        take_empty_set_events();
        assert!(
            solve_dielectric_modes(&mesh, &eps, &interior1, k0, 4)
                .unwrap()
                .is_empty()
        );
        assert!(
            solve_dielectric_modes2(&mesh, &eps, &interior2, k0, 4)
                .unwrap()
                .is_empty()
        );

        let (clad_r, outer_r) = (8.0 * a, 11.0 * a);
        let (mesh, tags, eps, interior) = pml_disk_913((n, n), a, (clad_r, outer_r), (3, 32));
        assert!(
            solve_dielectric_modes2_pml(&mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4)
                .unwrap()
                .is_empty()
        );
        // Each classifier evaluated the empty-set rule once: nothing reported.
        let events = take_empty_set_events();
        assert_eq!(events.len(), 3, "{events:?}");
        assert!(events.iter().all(|(_, pair)| pair.is_none()), "{events:?}");
    }

    /// One row of [`empty_set_rule_stress_sweep_913`]: classifier solves,
    /// those that returned no bound mode (empty-set rule evaluations),
    /// reports, and with-reference holes.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct SweepRow913 {
        solves: usize,
        evaluations: usize,
        reports: usize,
        reference_holes: usize,
    }

    /// Issue #913: a reduced, reproducible version of the stress sweep
    /// behind the empty-set thresholds, with the rule's reports recorded.
    ///
    /// Four fibers on two meshes, `n_modes = 4`:
    ///
    /// - two **anti-guides**, which guide nothing: SMF-28 contrast
    ///   reversed (`1.4447 / 1.4504`, `a = 4.1 µm`) and ~3 % contrast
    ///   reversed (`1.4447 / 1.4874`, `a = 1.4 µm`);
    /// - a **uniform** disk (`n = 1.4447`), whose guided window is empty;
    /// - the **thin core** [`THIN_CORE_913`], whose only guided mode sits at
    ///   an analytic `b = 4.7×10⁻⁹`, so that the empty set is the correct
    ///   result at any resolvable precision.
    ///
    /// Each runs the PEC p=1 and p=2 classifier attempts at Lanczos
    /// requests 4, 8 and 16 (16 is their own) and the PML attempt at
    /// requests 3 to 8, 12, 16 and 40 (40 is its own). 120 solves.
    ///
    /// What it asserts: the rule reports nothing on the anti-guides, the
    /// uniform disk or any PEC solve. Every report is on the thin-core PML
    /// solves and is the low-curl ladder pair of issue #947 (`b ≈ 0.42` on
    /// mesh `(4, 48)`), a known artifact and not a guided mode. That is why
    /// the rule only warns. No attempt returns an error from the empty-set
    /// branch. The per-row counts are printed (`--nocapture`).
    ///
    /// Measured (release, macOS aarch64; 14.6, 49, 54 and 63 s over four
    /// runs on a heavily loaded host): 120 solves, 87 empty-set
    /// evaluations, 5 reports. By fiber, evaluations / reports: anti-guides
    /// 34 / 0 (all PML; their PEC solves return box modes of the
    /// higher-index cladding, so the rule is not evaluated), uniform
    /// 30 / 0, thin core PEC 12 / 0, thin core PML 11 / 5. The 5 reports
    /// are mesh `(4, 48)` requests 6 and 7 (`b = 0.417`) and mesh `(6, 64)`
    /// requests 3, 5 and 6 (`b = 0.468`). The assertions pin the claims
    /// above, not these counts, which can shift with Lanczos round-off.
    ///
    /// The same sweep sees the with-reference #850 rule fire twice on the
    /// thin-core PML solves of mesh `(6, 64)` (the withheld ladder pair
    /// `β² = 35.259` above a returned ladder pair). That rule is unchanged
    /// here; the sweep counts it and it belongs to issue #947.
    #[test]
    #[ignore = "release-tier stress sweep: 120 classifier solves, 15 to 63 s in release over four runs (local, Apple M3 Ultra, host load average above 100), not timed in debug; run with `cargo test -p geode-core --release --lib -- --ignored --exact analytic::waveguide::tests::empty_set_rule_stress_sweep_913 --nocapture`"]
    fn empty_set_rule_stress_sweep_913() {
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        const PEC_REQUESTS: [usize; 3] = [4, 8, 16];
        const PML_REQUESTS: [usize; 9] = [3, 4, 5, 6, 7, 8, 12, 16, 40];
        const MESHES: [(usize, usize); 2] = [(4, 48), (6, 64)];
        // (name, (n_core, n_clad), core radius, PEC outer radius, PML box).
        type Fiber = (&'static str, (f64, f64), f64, f64, (f64, f64));
        let (thin_n, thin_a, thin_box) = THIN_CORE_913;
        let fibers: [Fiber; 4] = [
            (
                "anti-guide SMF-28",
                (1.4447, 1.4504),
                4.1,
                24.6,
                (32.8, 45.1),
            ),
            ("anti-guide ~3 %", (1.4447, 1.4874), 1.4, 8.4, (11.2, 15.4)),
            ("uniform", (1.4447, 1.4447), 1.4, 8.4, (11.2, 15.4)),
            ("thin core", thin_n, thin_a, 8.0, thin_box),
        ];

        // Tally one attempt: its hole-check outcome and the empty-set
        // events it recorded. Returns the reported pairs.
        let tally = |row: &mut SweepRow913, hole: Result<(), EigenError>| {
            row.solves += 1;
            match hole {
                Ok(()) => {}
                Err(EigenError::SelectionHole {
                    beta_sq_re,
                    lowest_returned,
                    ..
                }) => {
                    // Only the with-reference rule (#850) can error.
                    assert!(lowest_returned.is_finite(), "NaN reference in an error");
                    assert!(beta_sq_re > lowest_returned);
                    eprintln!(
                        "WITH-REFERENCE HOLE (#850 rule): withheld β² = {beta_sq_re:.5} above \
                         returned {lowest_returned:.5}"
                    );
                    row.reference_holes += 1;
                }
                Err(other) => panic!("unexpected error: {other:?}"),
            }
            let events = take_empty_set_events();
            assert!(events.len() <= 1, "{events:?}");
            let mut reported = Vec::new();
            for (_, pair) in events {
                row.evaluations += 1;
                if let Some(pair) = pair {
                    row.reports += 1;
                    reported.push(pair);
                }
            }
            reported
        };

        take_empty_set_events();
        let mut rows: Vec<(String, SweepRow913)> = Vec::new();
        // (mesh, request, b) of every PML report.
        let mut pml_reports: Vec<((usize, usize), usize, f64)> = Vec::new();
        for (name, (n_core, n_clad), a, pec_outer, (clad_r, outer_r)) in fibers {
            let eps_of = |tags: &[i32]| {
                epsilon_r_from_region_tags(tags, |t| {
                    if t == REGION_CORE {
                        n_core * n_core
                    } else {
                        n_clad * n_clad
                    }
                })
            };
            let (mut p1, mut p2, mut pml) = <(SweepRow913, SweepRow913, SweepRow913)>::default();
            for mesh_size in MESHES {
                let (mesh, tags) = disk_tri_mesh(a, pec_outer, mesh_size.0, mesh_size.1);
                let eps = eps_of(&tags);
                let (_, interior1) = disk_pec_interior_edges(&mesh, pec_outer);
                let interior2 = disk_pec_interior_dofs2(&mesh, pec_outer);
                for request in PEC_REQUESTS {
                    let (_, hole) =
                        solve_dielectric_modes_attempt(&mesh, &eps, &interior1, k0, 4, request)
                            .unwrap();
                    tally(&mut p1, hole);
                    let (_, hole) =
                        solve_dielectric_modes2_attempt(&mesh, &eps, &interior2, k0, 4, request)
                            .unwrap();
                    tally(&mut p2, hole);
                }

                let (mesh, tags, eps, interior) =
                    pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), mesh_size);
                let (e_max, e_min) = (
                    (n_core * n_core).max(n_clad * n_clad),
                    (n_core * n_core).min(n_clad * n_clad),
                );
                for request in PML_REQUESTS {
                    let (_, hole) = solve_dielectric_modes2_pml_attempt(
                        &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, request,
                    )
                    .unwrap();
                    for pair in tally(&mut pml, hole) {
                        let b = (pair.beta_sq.re - e_min * k0 * k0) / ((e_max - e_min) * k0 * k0);
                        eprintln!(
                            "REPORT {name} PML mesh {mesh_size:?} request {request}: β² = {:.5}, \
                             b = {b:.4}, ρ = {:.2e}, curl = {:.3e}",
                            pair.beta_sq.re, pair.residual, pair.curl_ratio
                        );
                        assert_eq!(name, "thin core", "a report on a fiber that guides nothing");
                        pml_reports.push((mesh_size, request, b));
                    }
                }
            }
            rows.push((format!("{name}, PEC p=1"), p1));
            rows.push((format!("{name}, PEC p=2"), p2));
            rows.push((format!("{name}, PML"), pml));
        }

        eprintln!(
            "\n#913 reduced stress sweep: solves / empty-set evaluations / reports / with-reference holes"
        );
        let mut total = SweepRow913::default();
        for (name, row) in &rows {
            eprintln!(
                "  {name:<28} {:>3} / {:>3} / {:>2} / {:>2}",
                row.solves, row.evaluations, row.reports, row.reference_holes
            );
            total.solves += row.solves;
            total.evaluations += row.evaluations;
            total.reports += row.reports;
            total.reference_holes += row.reference_holes;
        }
        eprintln!(
            "  {:<28} {:>3} / {:>3} / {:>2} / {:>2}",
            "total", total.solves, total.evaluations, total.reports, total.reference_holes
        );
        eprintln!("  thin-core PML reports (mesh, request, b): {pml_reports:?}");

        let row = |name: &str| rows.iter().find(|(n, _)| n == name).unwrap().1;
        // Fibers that guide nothing: evaluated, never reported.
        for fiber in ["anti-guide SMF-28", "anti-guide ~3 %", "uniform"] {
            for path in ["PEC p=1", "PEC p=2", "PML"] {
                let r = row(&format!("{fiber}, {path}"));
                assert_eq!(r.reports, 0, "{fiber}, {path}: {r:?}");
            }
            assert!(row(&format!("{fiber}, PML")).evaluations >= 12, "{fiber}");
        }
        // A uniform disk has no bound mode on any path: all 30 solves are
        // empty-set evaluations.
        for path in ["PEC p=1", "PEC p=2", "PML"] {
            let r = row(&format!("uniform, {path}"));
            assert_eq!(r.evaluations, r.solves, "uniform, {path}: {r:?}");
        }
        // Thin core, PEC: no bound mode (correct), nothing reported.
        for path in ["PEC p=1", "PEC p=2"] {
            let r = row(&format!("thin core, {path}"));
            assert_eq!((r.evaluations, r.reports), (r.solves, 0), "{path}: {r:?}");
        }
        // Thin core, PML: the only reports of the sweep, all of them the
        // #947 ladder pair (b ≈ 0.42 on mesh (4, 48), 0.47 on (6, 64);
        // the analytic LP01 is at b = 4.7e-9), including the request-6 case
        // of `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01`.
        assert_eq!(total.reports, row("thin core, PML").reports);
        assert!(
            pml_reports
                .iter()
                .any(|&(mesh_size, request, _)| mesh_size == (4, 48) && request == 6),
            "{pml_reports:?}"
        );
        for &(mesh_size, request, b) in &pml_reports {
            let expected = if mesh_size == (4, 48) { 0.417 } else { 0.468 };
            assert!(
                (b - expected).abs() < 2e-2,
                "mesh {mesh_size:?} request {request}: b = {b}"
            );
        }
        // Never at the PML classifier's own request.
        assert!(pml_reports.iter().all(|&(_, request, _)| request != 40));
    }
}
