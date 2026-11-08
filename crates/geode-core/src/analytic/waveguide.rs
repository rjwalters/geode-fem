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

mod cladding_bc;
mod dielectric;
mod dielectric_p2;
mod lp01_select;
mod nullspace_p2;
mod pml;

pub use cladding_bc::*;
pub use dielectric::*;
pub use dielectric_p2::*;
pub use lp01_select::*;
pub use nullspace_p2::*;
pub use pml::*;

#[cfg(test)]
mod tests;
