//! Driven Mie scattering benchmark machinery (issue #195, Epic #193):
//! scattered-field plane-wave source and `Q_ext` / `Q_sca` extraction
//! from a [`crate::driven::driven_solve`] solution.
//!
//! # Scattered-field formulation
//!
//! A plane wave `E_inc = x̂·exp(−i ω z)` (codebase `exp(+jωt)`
//! convention, natural units `c = μ₀ = ε₀ = 1`, so `ω ≡ k₀`)
//! illuminates the bundled `n = 1.5` dielectric sphere. Writing
//! `E_tot = E_inc + E_sca` and using that `E_inc` solves the vacuum
//! Helmholtz equation everywhere, the scattered field solves the
//! *driven* problem
//!
//! ```text
//! ∇×∇×E_sca − ω² ε E_sca = ω² (ε_r − 1) E_inc,
//! ```
//!
//! i.e. a volumetric polarization-current source supported only where
//! `ε_r ≠ 1` — the sphere interior. In the driven-solve convention
//! (`∇×∇×E − ω²εE = iωJ`, see [`crate::driven`]) the equivalent
//! current is `J = −iω (ε_r − 1) E_inc`, which
//! [`mie_polarization_source`] samples at tet centroids. The UPML
//! shell absorbs the outgoing scattered field; the total-field
//! equation is never posed inside the PML, so the formulation stays
//! exact.
//!
//! # Matched UPML — host oracle vs Burn path
//!
//! The eigenmode benchmarks' anisotropic UPML
//! ([`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`])
//! transforms **ε only** — the curl-curl stiffness keeps `μ = 1`. An
//! ε-only absorber is *not* impedance-matched: its interface
//! reflection is tolerable for locating eigenmode positions (the
//! benchmarks calibrate against PEC-cavity analytics, and the quasi
//! modes simply carry a finite Q), but it is fatal for a driven
//! scattering benchmark, where reflected power re-scatters off the
//! sphere and contaminates `Q` at the tens-of-percent level — observed
//! up to ~450 % error when driving on a quasi-cavity resonance.
//!
//! [`solve_scattered_field_matched_upml`] therefore assembles the
//! **full Sacks UPML** (Sacks et al., IEEE TAP 43, 1995): both
//! constitutive tensors are stretched,
//!
//! ```text
//! ε = ε_r · Λ,    μ = Λ,    Λ = s·I + (1/s − s)·r̂ r̂ᵀ,
//! s(r) = 1 − j σ(r)/ω,   σ(r) = σ₀·((r − R_PML_INNER)/d)²,
//! ```
//!
//! so the weak form gains a `Λ⁻¹` weight on the curl-curl term:
//! `A(ω) = K(Λ⁻¹) − ω² M(ε_r Λ)`. Because the lowest-order Nédélec
//! curls are constant per tet and `∫ λ_p λ_q dV = (V/20)(1 + δ_pq)`
//! is closed-form, both weighted matrices assemble exactly on the
//! host (CPU, f64) without quadrature. The factorization reuses the
//! same faer sparse LU as the driven solve.
//!
//! ## Burn path (issue #199) and the oracle decision: KEEP this solve
//!
//! The matched UPML is also expressible through the Burn batched-
//! kernel layer: [`build_matched_upml_materials`] evaluates the per-tet
//! `(ε_r·Λ, Λ⁻¹)` pair at tet centroids and
//! [`crate::driven::DrivenMaterials::MatchedUpml`] feeds it to
//! [`crate::driven::driven_solve`], which assembles `K(Λ⁻¹)` and
//! `M(ε_rΛ)` through the autodiff-preserving scatter path
//! ([`crate::nedelec_assembly::assemble_global_nedelec_with_full_tensors`]).
//! The Burn path and this host path agree at assembly precision for
//! the same `(σ₀, ω)` and the same per-tet-constant source
//! (`tests/mie_driven_scattering.rs`).
//!
//! **Recorded decision**: `solve_scattered_field_matched_upml` is
//! **kept** as an independent oracle rather than retired. It is the
//! only assembly-independent cross-check of the Burn tensor-weighted
//! kernels (closed-form host f64 assembly vs batched Burn scatter),
//! exactly the role the CPU reference kernels play for the scalar
//! path. Production/differentiable callers should prefer the Burn
//! path via `driven_solve`.
//!
//! # Efficiency extraction — recorded choice (issue #195)
//!
//! Two **independent** extractions, per the issue's "pick the simpler
//! first and record the choice":
//!
//! - **`Q_ext` via the volume form of the optical theorem**
//!   ([`extinction_power`]). For time-harmonic fields the extinction
//!   power equals the work the incident field does on the polarization
//!   current,
//!
//!   ```text
//!   P_ext = (ω/2) ∫_sphere (ε_r − 1) · Im[ E_inc · E_sca* ] dV,
//!   ```
//!
//!   mathematically equivalent (by reciprocity) to the
//!   forward-scattering-amplitude form but needing **no far-field
//!   transform** — only the Whitney interpolant of `E_sca` inside the
//!   sphere, where the FEM solution is best resolved. This is the
//!   "simpler first" choice over a near-to-far-field transform.
//!
//! - **`Q_sca` via direct Poynting-flux integration**
//!   ([`scattered_flux_power`]) of `½ Re(E_sca × H_sca*) · n̂` over the
//!   closed polyhedral surface separating tets with centroid radius
//!   `< r_obs` from the rest, with `r_obs` in the vacuum gap between
//!   sphere and PML. `H_sca = (i/ω) ∇×E_sca` is piecewise constant and
//!   `E_sca` piecewise linear for lowest-order Nédélec, so one-point
//!   (face-centroid) quadrature integrates each face exactly.
//!
//! For the lossless sphere `Q_ext = Q_sca` analytically; numerically
//! the two extractions differ (volume overlap vs surface flux), so
//! their agreement is itself a discretization health check.
//!
//! Efficiencies are powers normalized by the geometric cross section
//! times the incident irradiance: `Q = P / (½ |E₀|² π a²)` with
//! `|E₀| = 1` and impedance 1 in natural units ([`q_from_power`]).
//!
//! The analytic oracle is [`crate::mie_scattering::mie_efficiencies`].

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use crate::driven::{CurrentSource, DrivenError, DrivenSolution};
use crate::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES};
use crate::TetMesh;

/// Incident plane wave `E_inc(x) = x̂ · exp(−i ω z)` (unit amplitude,
/// `x̂`-polarized, propagating along `+z` under `exp(+jωt)`).
pub fn plane_wave_e_inc(omega: f64, p: [f64; 3]) -> [c64; 3] {
    let phase = -omega * p[2];
    [
        c64::new(phase.cos(), phase.sin()),
        c64::new(0.0, 0.0),
        c64::new(0.0, 0.0),
    ]
}

/// Scattered-field polarization-current source
/// `J = −iω (ε_r − 1) E_inc` sampled at tet centroids, supported on
/// tets whose physical tag equals `interior_tag`
/// ([`crate::mesh::PHYS_SPHERE_INTERIOR`] for the bundled fixture)
/// where `ε_r = n_inside²`.
pub fn mie_polarization_source(
    mesh: &TetMesh,
    tet_physical_tags: &[i32],
    interior_tag: i32,
    n_inside: f64,
    omega: f64,
) -> CurrentSource {
    assert_eq!(
        tet_physical_tags.len(),
        mesh.n_tets(),
        "one physical tag per tet"
    );
    let contrast = n_inside * n_inside - 1.0;
    let centroids = crate::nedelec_assembly::tet_centroids(mesh);
    let j_tet = centroids
        .iter()
        .zip(tet_physical_tags.iter())
        .map(|(c, &tag)| {
            if tag == interior_tag {
                let e_inc = plane_wave_e_inc(omega, *c);
                // J = −iω(ε_r − 1) E_inc.
                let scale = c64::new(0.0, -omega * contrast);
                [scale * e_inc[0], scale * e_inc[1], scale * e_inc[2]]
            } else {
                [c64::new(0.0, 0.0); 3]
            }
        })
        .collect();
    CurrentSource { j_tet }
}

/// Per-tet affine geometry: barycentric gradients and volume.
struct TetGeometry {
    /// Vertex coordinates.
    verts: [[f64; 3]; 4],
    /// `∇λ_i`, constant over the tet.
    grad: [[f64; 3]; 4],
    /// Unsigned volume.
    volume: f64,
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn tet_geometry(mesh: &TetMesh, tet: &[u32; 4]) -> TetGeometry {
    let verts = [
        mesh.nodes[tet[0] as usize],
        mesh.nodes[tet[1] as usize],
        mesh.nodes[tet[2] as usize],
        mesh.nodes[tet[3] as usize],
    ];
    let e1 = sub(verts[1], verts[0]);
    let e2 = sub(verts[2], verts[0]);
    let e3 = sub(verts[3], verts[0]);
    let det = dot(e1, cross(e2, e3));
    // Face-normal forms of the gradients: ∇λ_i = (opposite-face normal)
    // scaled so that λ_i is 1 at vertex i, 0 on the opposite face.
    let g1 = cross(e2, e3);
    let g2 = cross(e3, e1);
    let g3 = cross(e1, e2);
    let inv = 1.0 / det;
    let grad1 = [g1[0] * inv, g1[1] * inv, g1[2] * inv];
    let grad2 = [g2[0] * inv, g2[1] * inv, g2[2] * inv];
    let grad3 = [g3[0] * inv, g3[1] * inv, g3[2] * inv];
    let grad0 = [
        -(grad1[0] + grad2[0] + grad3[0]),
        -(grad1[1] + grad2[1] + grad3[1]),
        -(grad1[2] + grad2[2] + grad3[2]),
    ];
    TetGeometry {
        verts,
        grad: [grad0, grad1, grad2, grad3],
        volume: det.abs() / 6.0,
    }
}

/// Sign-folded local edge DOFs `d_e = sign_e · e_edges[idx_e]` for one
/// tet, in [`TET_LOCAL_EDGES`] order.
fn local_dofs(tet_edge_row: &[(u32, i8); 6], e_edges: &[c64]) -> [c64; 6] {
    std::array::from_fn(|e| {
        let (idx, sign) = tet_edge_row[e];
        e_edges[idx as usize] * c64::new(sign as f64, 0.0)
    })
}

/// Whitney 1-form interpolant `E(x) = Σ_e d_e (λ_a ∇λ_b − λ_b ∇λ_a)`
/// at barycentric coordinates `lambda` inside the tet.
fn eval_field_at_bary(geom: &TetGeometry, dofs: &[c64; 6], lambda: [f64; 4]) -> [c64; 3] {
    let mut e = [c64::new(0.0, 0.0); 3];
    for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        let d = dofs[slot];
        for (k, e_k) in e.iter_mut().enumerate() {
            let w = lambda[a] * geom.grad[b][k] - lambda[b] * geom.grad[a][k];
            *e_k += d * c64::new(w, 0.0);
        }
    }
    e
}

/// Piecewise-constant curl `∇×E = Σ_e d_e · 2 (∇λ_a × ∇λ_b)`.
fn eval_curl(geom: &TetGeometry, dofs: &[c64; 6]) -> [c64; 3] {
    let mut c = [c64::new(0.0, 0.0); 3];
    for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        let d = dofs[slot];
        let w = cross(geom.grad[a], geom.grad[b]);
        for k in 0..3 {
            c[k] += d * c64::new(2.0 * w[k], 0.0);
        }
    }
    c
}

/// Degree-2 4-point tet quadrature (barycentric points, equal weights
/// `V/4`). Shared with the Burn-side quadrature RHS kernel
/// ([`crate::nedelec::batched_nedelec_local_rhs_quad4`]) so the host
/// and Burn paths integrate spatially varying sources identically.
const QUAD_A: f64 = crate::nedelec::TET_QUAD4_A;
const QUAD_B: f64 = crate::nedelec::TET_QUAD4_B;

/// Extinction power via the volume optical theorem,
///
/// ```text
/// P_ext = (ω/2) ∫_sphere (ε_r − 1) · Im[ E_inc · E_sca* ] dV,
/// ```
///
/// evaluated with the Whitney interpolant of `e_edges` (full-length
/// edge-DOF vector from [`crate::driven::driven_solve`]) and a
/// degree-2 four-point tet quadrature for the `E_inc` phase variation.
pub fn extinction_power(
    mesh: &TetMesh,
    tet_physical_tags: &[i32],
    interior_tag: i32,
    n_inside: f64,
    omega: f64,
    e_edges: &[c64],
) -> f64 {
    assert_eq!(tet_physical_tags.len(), mesh.n_tets());
    let edges = mesh.edges();
    assert_eq!(e_edges.len(), edges.len(), "one DOF per global edge");
    let tet_edges = mesh.tet_edges();

    let contrast = n_inside * n_inside - 1.0;
    let mut p_ext = 0.0_f64;
    for (t, tet) in mesh.tets.iter().enumerate() {
        if tet_physical_tags[t] != interior_tag {
            continue;
        }
        let geom = tet_geometry(mesh, tet);
        let dofs = local_dofs(&tet_edges[t], e_edges);
        let w_q = geom.volume / 4.0;
        for q in 0..4 {
            let lambda: [f64; 4] = std::array::from_fn(|i| if i == q { QUAD_A } else { QUAD_B });
            let x_q: [f64; 3] = std::array::from_fn(|k| {
                lambda[0] * geom.verts[0][k]
                    + lambda[1] * geom.verts[1][k]
                    + lambda[2] * geom.verts[2][k]
                    + lambda[3] * geom.verts[3][k]
            });
            let e_sca = eval_field_at_bary(&geom, &dofs, lambda);
            let e_inc = plane_wave_e_inc(omega, x_q);
            // u = E_inc · E_sca* (unconjugated dot on E_inc).
            let u = e_inc[0] * e_sca[0].conj()
                + e_inc[1] * e_sca[1].conj()
                + e_inc[2] * e_sca[2].conj();
            p_ext += w_q * u.im;
        }
    }
    0.5 * omega * contrast * p_ext
}

/// Scattered power via direct Poynting-flux integration of
/// `½ Re(E_sca × H_sca*) · n̂` over the closed polyhedral surface
/// bounding the union of tets with centroid radius `< r_obs`
/// (`r_obs` must lie in the lossless vacuum gap between the sphere
/// surface and the PML inner radius so the enclosed region contains
/// the whole source and no absorber).
///
/// With `H = (i/ω) ∇×E` (from `∇×E = −iωH`, `exp(+jωt)`), the
/// per-face integrand reduces to `(1/2ω)·Im[(E × (∇×E)*) · n̂]`, which
/// is linear over each face and integrated exactly by face-centroid
/// quadrature. Fields are taken from the *inside* tet of each face.
pub fn scattered_flux_power(mesh: &TetMesh, omega: f64, e_edges: &[c64], r_obs: f64) -> f64 {
    use std::collections::HashMap;

    let edges = mesh.edges();
    assert_eq!(e_edges.len(), edges.len(), "one DOF per global edge");
    let tet_edges = mesh.tet_edges();
    let centroids = crate::nedelec_assembly::tet_centroids(mesh);

    let inside: Vec<bool> = centroids
        .iter()
        .map(|c| dot(*c, *c).sqrt() < r_obs)
        .collect();
    assert!(
        inside.iter().any(|&b| b),
        "no tets with centroid radius < {r_obs}"
    );
    assert!(
        inside.iter().any(|&b| !b),
        "all tets inside r_obs = {r_obs}; surface is empty"
    );

    // Face key (sorted vertex triple) → how many *inside* and *total*
    // adjacent tets.
    let mut face_count: HashMap<(u32, u32, u32), (u8, u8)> = HashMap::new();
    for (t, tet) in mesh.tets.iter().enumerate() {
        for lf in &TET_LOCAL_FACES {
            let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
            tri.sort_unstable();
            let entry = face_count.entry((tri[0], tri[1], tri[2])).or_insert((0, 0));
            if inside[t] {
                entry.0 += 1;
            }
            entry.1 += 1;
        }
    }

    let mut p_flux = 0.0_f64;
    for (t, tet) in mesh.tets.iter().enumerate() {
        if !inside[t] {
            continue;
        }
        let geom = tet_geometry(mesh, tet);
        let dofs = local_dofs(&tet_edges[t], e_edges);
        let curl = eval_curl(&geom, &dofs);

        for (local_face, lf) in TET_LOCAL_FACES.iter().enumerate() {
            let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
            tri.sort_unstable();
            let &(n_inside_adj, _n_total) = face_count
                .get(&(tri[0], tri[1], tri[2]))
                .expect("face derived from tet must be counted");
            // Surface faces have exactly one inside tet adjacent. (An
            // outer-boundary face owned by a single inside tet would
            // also match, but r_obs < R_BUFFER makes that impossible.)
            if n_inside_adj != 1 {
                continue;
            }

            let p0 = mesh.nodes[tet[lf[0]] as usize];
            let p1 = mesh.nodes[tet[lf[1]] as usize];
            let p2 = mesh.nodes[tet[lf[2]] as usize];
            let mut normal = cross(sub(p1, p0), sub(p2, p0));
            let area = 0.5 * dot(normal, normal).sqrt();
            // Orient outward: away from the opposite vertex (the local
            // vertex not on this face, i.e. local index `local_face`).
            let opposite = geom.verts[local_face];
            let face_centroid: [f64; 3] = std::array::from_fn(|k| (p0[k] + p1[k] + p2[k]) / 3.0);
            if dot(normal, sub(face_centroid, opposite)) < 0.0 {
                normal = [-normal[0], -normal[1], -normal[2]];
            }
            let n_len = dot(normal, normal).sqrt();
            let n_hat = [normal[0] / n_len, normal[1] / n_len, normal[2] / n_len];

            // Barycentric coordinates of the face centroid: 1/3 on the
            // three face vertices, 0 on the opposite one.
            let mut lambda = [0.0_f64; 4];
            for &lv in lf {
                lambda[lv] = 1.0 / 3.0;
            }
            let e = eval_field_at_bary(&geom, &dofs, lambda);

            // (E × C*) · n̂ with C = ∇×E.
            let cc = [curl[0].conj(), curl[1].conj(), curl[2].conj()];
            let exc = [
                e[1] * cc[2] - e[2] * cc[1],
                e[2] * cc[0] - e[0] * cc[2],
                e[0] * cc[1] - e[1] * cc[0],
            ];
            let s_n = (exc[0] * c64::new(n_hat[0], 0.0)
                + exc[1] * c64::new(n_hat[1], 0.0)
                + exc[2] * c64::new(n_hat[2], 0.0))
            .im / (2.0 * omega);
            p_flux += area * s_n;
        }
    }
    p_flux
}

/// Normalize a power to a Mie efficiency:
/// `Q = P / (S_inc · π a²)` with incident irradiance
/// `S_inc = ½ |E₀|² = ½` in natural units (`|E₀| = 1`, impedance 1).
pub fn q_from_power(power: f64, sphere_radius: f64) -> f64 {
    power / (0.5 * std::f64::consts::PI * sphere_radius * sphere_radius)
}

/// Full Sacks UPML constitutive tensors `(Λ, Λ⁻¹)` at a point, for the
/// bundled fixture's radial shell geometry (`R_PML_INNER` → `R_BUFFER`,
/// quadratic σ ramp — the same profile family as
/// [`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`]).
///
/// ```text
/// Λ   = s·I + (1/s − s)·r̂ r̂ᵀ        (radial eigenvalue 1/s, transverse s)
/// Λ⁻¹ = (1/s)·I + (s − 1/s)·r̂ r̂ᵀ
/// s   = 1 − j σ₀ ((r − R_PML_INNER)/d)² / ω
/// ```
///
/// Unlike the ε-only diagonal builder, the off-diagonal entries of the
/// Cartesian tensor are kept — the host assembly sandwiches the full
/// 3×3 tensor, so there is no diagonal-restriction approximation.
/// Outside the shell (`r ≤ R_PML_INNER`) both tensors are the identity.
pub fn upml_matched_tensors(
    p: [f64; 3],
    sigma_0: f64,
    omega: f64,
) -> ([[c64; 3]; 3], [[c64; 3]; 3]) {
    use crate::mesh::{R_BUFFER, R_PML_INNER};
    let r = dot(p, p).sqrt();
    let mut lam = [[c64::new(0.0, 0.0); 3]; 3];
    let mut lam_inv = [[c64::new(0.0, 0.0); 3]; 3];
    if r <= R_PML_INNER {
        for k in 0..3 {
            lam[k][k] = c64::new(1.0, 0.0);
            lam_inv[k][k] = c64::new(1.0, 0.0);
        }
        return (lam, lam_inv);
    }
    let u = ((r - R_PML_INNER) / (R_BUFFER - R_PML_INNER)).clamp(0.0, 1.0);
    let sigma = sigma_0 * u * u;
    let s = c64::new(1.0, -sigma / omega.max(1e-12));
    let s_inv = c64::new(1.0, 0.0) / s;
    let r_hat = p.map(|x| x / r);
    for i in 0..3 {
        for j in 0..3 {
            let delta = if i == j { 1.0 } else { 0.0 };
            let rr = c64::new(r_hat[i] * r_hat[j], 0.0);
            lam[i][j] = s * c64::new(delta, 0.0) + (s_inv - s) * rr;
            lam_inv[i][j] = s_inv * c64::new(delta, 0.0) + (s - s_inv) * rr;
        }
    }
    (lam, lam_inv)
}

/// Per-tet matched-UPML constitutive tensors for the Burn assembly
/// path (issue #199): evaluates [`upml_matched_tensors`] at each tet
/// centroid and returns `(ε, ν)` with `ε = ε_r·Λ` (mass weight) and
/// `ν = Λ⁻¹` (curl-curl weight) — exactly the per-tet inputs the host
/// path [`solve_scattered_field_matched_upml`] uses internally, so a
/// [`crate::driven::driven_solve`] call with
/// [`crate::driven::DrivenMaterials::MatchedUpml`] is a pure
/// assembly-equivalence counterpart of the host solve.
///
/// - `ε_r = n_inside²` on tets tagged `interior_tag`, 1 elsewhere
///   (the PML stretch multiplies on top).
/// - `Λ ≠ I` only on tets tagged [`crate::mesh::PHYS_PML_SHELL`] with
///   centroid radius beyond `R_PML_INNER` (identity in the interior /
///   vacuum gap, and for `σ₀ = 0`).
///
/// `tet_physical_tags.len()` must equal the number of tets in `mesh`.
#[allow(clippy::type_complexity)]
pub fn build_matched_upml_materials(
    mesh: &TetMesh,
    tet_physical_tags: &[i32],
    interior_tag: i32,
    n_inside: f64,
    sigma_0: f64,
    omega: f64,
) -> (Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>) {
    assert_eq!(
        tet_physical_tags.len(),
        mesh.n_tets(),
        "one physical tag per tet"
    );
    let centroids = crate::nedelec_assembly::tet_centroids(mesh);
    let identity = {
        let mut w = [[c64::new(0.0, 0.0); 3]; 3];
        for (k, row) in w.iter_mut().enumerate() {
            row[k] = c64::new(1.0, 0.0);
        }
        w
    };

    let mut eps_tensor = Vec::with_capacity(mesh.n_tets());
    let mut nu_tensor = Vec::with_capacity(mesh.n_tets());
    for (c, &tag) in centroids.iter().zip(tet_physical_tags.iter()) {
        let eps_r = if tag == interior_tag {
            n_inside * n_inside
        } else {
            1.0
        };
        let (lam, lam_inv) = if tag == crate::mesh::PHYS_PML_SHELL {
            upml_matched_tensors(*c, sigma_0, omega)
        } else {
            (identity, identity)
        };
        let eps = lam.map(|row| row.map(|v| v * c64::new(eps_r, 0.0)));
        eps_tensor.push(eps);
        nu_tensor.push(lam_inv);
    }
    (eps_tensor, nu_tensor)
}

/// Quadratic form `aᵀ W b` for a complex 3×3 tensor and real vectors.
fn sandwich(w: &[[c64; 3]; 3], a: [f64; 3], b: [f64; 3]) -> c64 {
    let mut acc = c64::new(0.0, 0.0);
    for (i, row) in w.iter().enumerate() {
        for (j, wij) in row.iter().enumerate() {
            acc += *wij * c64::new(a[i] * b[j], 0.0);
        }
    }
    acc
}

/// Scattered-field driven solve with the **matched** (full Sacks)
/// UPML: `A(ω) x = b` with `A(ω) = K(Λ⁻¹) − ω² M(ε_r Λ)` assembled
/// exactly on the host. Kept as the **independent oracle** for the
/// Burn-path matched UPML
/// ([`crate::driven::DrivenMaterials::MatchedUpml`] +
/// [`build_matched_upml_materials`]) — see the module docs for the
/// recorded retire-vs-keep decision.
///
/// - `pec_interior_mask` — per-edge keep mask over `mesh.edges()`
///   order (e.g. from
///   [`crate::nedelec_assembly::sphere_pec_interior_edges`]).
/// - `n_inside` — sphere refractive index; `ε_r = n_inside²` on tets
///   tagged `interior_tag`, 1 elsewhere (the PML stretch multiplies
///   on top).
/// - `sigma_0` — UPML strength (quadratic profile). The driven Mie
///   benchmark uses `σ₀ = 25` (calibrated; round-trip continuum
///   attenuation `exp(−2σ₀d/3) ≈ 2·10⁻⁴`).
/// - `j_at(tet, x)` — current density at quadrature point `x` inside
///   tet `tet`. The RHS `b_i = iω ∫ N_i · J dV` is integrated with a
///   degree-2 four-point rule, which is exact for per-tet-constant `J`
///   (matching [`crate::driven::driven_solve`]'s RHS bit-for-bit in
///   that case) and captures the plane-wave phase variation of the
///   scattered-field source.
///
/// Returns the same [`DrivenSolution`] shape as the Burn-path driven
/// solve (full-length edge vector, zeros on PEC edges, post-solve
/// relative residual).
// The argument list mirrors the physical problem statement (mesh +
// material tags + BCs + frequency + PML strength + source); folding it
// into a one-shot params struct would only add API noise for a single
// benchmark-facing entry point.
#[allow(clippy::too_many_arguments)]
pub fn solve_scattered_field_matched_upml(
    mesh: &TetMesh,
    tet_physical_tags: &[i32],
    interior_tag: i32,
    pec_interior_mask: &[bool],
    n_inside: f64,
    sigma_0: f64,
    omega: f64,
    j_at: impl Fn(usize, [f64; 3]) -> [c64; 3],
) -> Result<DrivenSolution, DrivenError> {
    use crate::complex_lanczos::{solve_with_lu, spmv};

    let n_tets = mesh.n_tets();
    if tet_physical_tags.len() != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: tet_physical_tags.len(),
            want: n_tets,
        });
    }
    let edges = mesh.edges();
    let n_edges = edges.len();
    if pec_interior_mask.len() != n_edges {
        return Err(DrivenError::MaskDimMismatch {
            got: pec_interior_mask.len(),
            want: n_edges,
        });
    }
    let tet_edges = mesh.tet_edges();
    let centroids = crate::nedelec_assembly::tet_centroids(mesh);

    // Remap full edge indices → contiguous interior indices.
    let mut remap = vec![-1_i64; n_edges];
    let mut n_interior = 0_usize;
    for (i, &keep) in pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n_interior as i64;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }

    let omega2 = omega * omega;
    let identity = {
        let mut w = [[c64::new(0.0, 0.0); 3]; 3];
        for (k, row) in w.iter_mut().enumerate() {
            row[k] = c64::new(1.0, 0.0);
        }
        w
    };

    let mut triplets: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(36 * n_tets);
    let mut b_full = vec![c64::new(0.0, 0.0); n_edges];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let geom = tet_geometry(mesh, tet);
        let tag = tet_physical_tags[t];
        let eps_r = if tag == interior_tag {
            n_inside * n_inside
        } else {
            1.0
        };
        let in_pml = tag == crate::mesh::PHYS_PML_SHELL;
        let (lam, lam_inv) = if in_pml {
            upml_matched_tensors(centroids[t], sigma_0, omega)
        } else {
            (identity, identity)
        };

        // Constant per-tet curls 2(∇λ_a × ∇λ_b) in local edge order.
        let curls: [[f64; 3]; 6] = std::array::from_fn(|e| {
            let (a, b) = TET_LOCAL_EDGES[e];
            cross(geom.grad[a], geom.grad[b]).map(|x| 2.0 * x)
        });

        // Quadrature points (degree-2 rule) for the RHS.
        let quad_lambda: [[f64; 4]; 4] =
            std::array::from_fn(|q| std::array::from_fn(|i| if i == q { QUAD_A } else { QUAD_B }));
        let quad_x: [[f64; 3]; 4] = std::array::from_fn(|q| {
            std::array::from_fn(|k| {
                (0..4)
                    .map(|v| quad_lambda[q][v] * geom.verts[v][k])
                    .sum::<f64>()
            })
        });
        let quad_j: [[c64; 3]; 4] = std::array::from_fn(|q| j_at(t, quad_x[q]));

        for i in 0..6 {
            let (ia, ib) = TET_LOCAL_EDGES[i];
            let (gi, si) = tet_edges[t][i];
            let si = si as f64;

            // RHS: b_i = iω ∫ N_i · J dV, four-point quadrature.
            let mut bi = c64::new(0.0, 0.0);
            for q in 0..4 {
                let lamq = quad_lambda[q];
                let n_i: [f64; 3] = std::array::from_fn(|k| {
                    lamq[ia] * geom.grad[ib][k] - lamq[ib] * geom.grad[ia][k]
                });
                bi += (quad_j[q][0] * c64::new(n_i[0], 0.0)
                    + quad_j[q][1] * c64::new(n_i[1], 0.0)
                    + quad_j[q][2] * c64::new(n_i[2], 0.0))
                    * c64::new(geom.volume / 4.0, 0.0);
            }
            b_full[gi as usize] += c64::new(0.0, omega) * bi * c64::new(si, 0.0);

            let ri = remap[gi as usize];
            if ri < 0 {
                continue;
            }
            for j in 0..6 {
                let (ja, jb) = TET_LOCAL_EDGES[j];
                let (gj, sj) = tet_edges[t][j];
                let rj = remap[gj as usize];
                if rj < 0 {
                    continue;
                }
                let sj = sj as f64;

                // K_ij = V · c_iᵀ Λ⁻¹ c_j (constant curls).
                let k_ij = sandwich(&lam_inv, curls[i], curls[j]) * c64::new(geom.volume, 0.0);
                // M_ij = (ε_r V/20)·[(1+δ_ac) g_bᵀΛg_d − (1+δ_ad) g_bᵀΛg_c
                //                  − (1+δ_bc) g_aᵀΛg_d + (1+δ_bd) g_aᵀΛg_c].
                let dl = |p: usize, q: usize| if p == q { 2.0 } else { 1.0 };
                let m_ij = (sandwich(&lam, geom.grad[ib], geom.grad[jb])
                    * c64::new(dl(ia, ja), 0.0)
                    - sandwich(&lam, geom.grad[ib], geom.grad[ja]) * c64::new(dl(ia, jb), 0.0)
                    - sandwich(&lam, geom.grad[ia], geom.grad[jb]) * c64::new(dl(ib, ja), 0.0)
                    + sandwich(&lam, geom.grad[ia], geom.grad[ja]) * c64::new(dl(ib, jb), 0.0))
                    * c64::new(eps_r * geom.volume / 20.0, 0.0);

                let a_val = (k_ij - m_ij * c64::new(omega2, 0.0)) * c64::new(si * sj, 0.0);
                triplets.push(Triplet::new(ri as usize, rj as usize, a_val));
            }
        }
    }

    let a_int =
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, &triplets)
            .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))?;
    let b_int: Vec<c64> = pec_interior_mask
        .iter()
        .zip(b_full.iter())
        .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
        .collect();

    let lu = a_int
        .as_ref()
        .sp_lu()
        .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
    let mut x_int = vec![c64::new(0.0, 0.0); n_interior];
    solve_with_lu(&lu, &b_int, &mut x_int).map_err(|e| DrivenError::Solve(format!("{e}")))?;

    // Post-solve residual check (same health metric as driven_solve).
    let mut ax = vec![c64::new(0.0, 0.0); n_interior];
    spmv(a_int.as_ref(), &x_int, &mut ax);
    let mut res2 = 0.0_f64;
    let mut b2 = 0.0_f64;
    for i in 0..n_interior {
        let r = ax[i] - b_int[i];
        res2 += r.norm_sqr();
        b2 += b_int[i].norm_sqr();
    }
    let residual_rel = if b2 > 0.0 {
        (res2 / b2).sqrt()
    } else {
        res2.sqrt()
    };

    let mut e_edges = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            e_edges[full_idx] = x_int[ri as usize];
        }
    }

    Ok(DrivenSolution {
        e_edges,
        n_interior,
        residual_rel,
    })
}

/// Plane-wave polarization-current density closure for
/// [`solve_scattered_field_matched_upml`]:
/// `J(x) = −iω (n² − 1) E_inc(x)` on tets tagged `interior_tag`, zero
/// elsewhere — the same source as [`mie_polarization_source`] but
/// evaluated at quadrature points instead of centroids.
pub fn plane_wave_polarization_current<'a>(
    tet_physical_tags: &'a [i32],
    interior_tag: i32,
    n_inside: f64,
    omega: f64,
) -> impl Fn(usize, [f64; 3]) -> [c64; 3] + 'a {
    let scale = c64::new(0.0, -omega * (n_inside * n_inside - 1.0));
    move |tet: usize, x: [f64; 3]| {
        if tet_physical_tags[tet] == interior_tag {
            let e_inc = plane_wave_e_inc(omega, x);
            [scale * e_inc[0], scale * e_inc[1], scale * e_inc[2]]
        } else {
            [c64::new(0.0, 0.0); 3]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_tet_mesh;

    /// A constant vector field is exactly representable in the Whitney
    /// space: `d_e = E₀ · (p_b − p_a)` along the canonical (global)
    /// edge direction. The interpolant must reproduce `E₀` at any
    /// interior point with zero curl.
    #[test]
    fn whitney_interpolant_reproduces_constant_field() {
        let mesh = cube_tet_mesh(2, 1.0);
        let e0 = [0.3_f64, -1.1, 0.7];
        let edges = mesh.edges();
        let e_edges: Vec<c64> = edges
            .iter()
            .map(|e| {
                let p = mesh.nodes[e[0] as usize];
                let q = mesh.nodes[e[1] as usize];
                c64::new(dot(e0, sub(q, p)), 0.0)
            })
            .collect();
        let tet_edges = mesh.tet_edges();
        for (t, tet) in mesh.tets.iter().enumerate() {
            let geom = tet_geometry(&mesh, tet);
            let dofs = local_dofs(&tet_edges[t], &e_edges);
            let lambda = [0.25_f64; 4];
            let e = eval_field_at_bary(&geom, &dofs, lambda);
            for k in 0..3 {
                assert!(
                    (e[k].re - e0[k]).abs() < 1e-12 && e[k].im.abs() < 1e-12,
                    "tet {t}, component {k}: got {:?}, want {}",
                    e[k],
                    e0[k]
                );
            }
            let c = eval_curl(&geom, &dofs);
            for (k, c_k) in c.iter().enumerate() {
                assert!(
                    c_k.norm() < 1e-10,
                    "constant field must be curl-free; tet {t} curl[{k}] = {c_k:?}"
                );
            }
        }
    }

    /// Barycentric gradients sum to zero and satisfy
    /// `∇λ_i · (p_j − p_0) = δ_ij` structure.
    #[test]
    fn tet_geometry_gradients_are_dual_basis() {
        let mesh = cube_tet_mesh(1, 1.0);
        for tet in &mesh.tets {
            let geom = tet_geometry(&mesh, tet);
            for i in 0..4 {
                for j in 0..4 {
                    // λ_i(vertex j) = δ_ij; with λ_i affine this means
                    // ∇λ_i · (p_j − p_i) = δ_ij − 1 for j ≠ i… simplest
                    // check: ∇λ_i · (p_j − p_k) = δ_ij − δ_ik.
                    let k = (j + 1) % 4;
                    let want = (i == j) as i32 as f64 - (i == k) as i32 as f64;
                    let got = dot(geom.grad[i], sub(geom.verts[j], geom.verts[k]));
                    assert!(
                        (got - want).abs() < 1e-12,
                        "grad[{i}]·(v{j}−v{k}) = {got}, want {want}"
                    );
                }
            }
            assert!(geom.volume > 0.0);
        }
    }

    /// The linear-in-`x` Whitney field `E = g_b λ_a − g_a λ_b` has a
    /// known constant curl; cross-check `eval_curl` against a central
    /// finite-difference of `eval_field_at_bary` along one axis.
    #[test]
    fn curl_matches_finite_difference() {
        let mesh = cube_tet_mesh(1, 1.0);
        let tet_edges = mesh.tet_edges();
        let edges = mesh.edges();
        // An arbitrary smooth-ish DOF vector.
        let e_edges: Vec<c64> = (0..edges.len())
            .map(|i| c64::new((i as f64 * 0.37).sin(), (i as f64 * 0.21).cos()))
            .collect();
        let t = 0_usize;
        let geom = tet_geometry(&mesh, &mesh.tets[t]);
        let dofs = local_dofs(&tet_edges[t], &e_edges);
        let curl = eval_curl(&geom, &dofs);

        // Finite-difference the interpolant in *Cartesian* coordinates
        // around the barycenter: λ(x + h ê_k) = λ(x) + h ∇λ·ê_k.
        let h = 1e-6_f64;
        let lam0 = [0.25_f64; 4];
        let field_at = |delta: [f64; 3]| -> [c64; 3] {
            let lam: [f64; 4] = std::array::from_fn(|i| lam0[i] + dot(geom.grad[i], delta));
            eval_field_at_bary(&geom, &dofs, lam)
        };
        let dx = |k: usize, comp: usize| -> c64 {
            let mut dp = [0.0; 3];
            dp[k] = h;
            let mut dm = [0.0; 3];
            dm[k] = -h;
            (field_at(dp)[comp] - field_at(dm)[comp]) / c64::new(2.0 * h, 0.0)
        };
        // curl = (∂yEz−∂zEy, ∂zEx−∂xEz, ∂xEy−∂yEx).
        let fd = [
            dx(1, 2) - dx(2, 1),
            dx(2, 0) - dx(0, 2),
            dx(0, 1) - dx(1, 0),
        ];
        for k in 0..3 {
            assert!(
                (curl[k] - fd[k]).norm() < 1e-6,
                "curl[{k}] analytic {:?} vs FD {:?}",
                curl[k],
                fd[k]
            );
        }
    }

    /// `Λ · Λ⁻¹ = I` inside the shell; both reduce to the identity
    /// outside it and for σ₀ = 0; the radial/transverse eigenstructure
    /// is `(1/s, s, s)`.
    #[test]
    #[allow(clippy::needless_range_loop)] // 3×3 tensor index algebra reads clearer indexed
    fn upml_matched_tensors_are_consistent() {
        let omega = 1.7;
        let sigma_0 = 25.0;
        // Inside the gap: identity.
        let (lam, lam_inv) = upml_matched_tensors([0.6, 0.3, -0.2], sigma_0, omega);
        for i in 0..3 {
            for j in 0..3 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert_eq!(lam[i][j], c64::new(want, 0.0));
                assert_eq!(lam_inv[i][j], c64::new(want, 0.0));
            }
        }
        // In the shell: Λ·Λ⁻¹ = I, radial eigenvector r̂ with value 1/s.
        let p: [f64; 3] = [1.2, -0.9, 0.8]; // |p| ≈ 1.70, inside the shell
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        assert!(r > crate::mesh::R_PML_INNER && r < crate::mesh::R_BUFFER);
        let (lam, lam_inv) = upml_matched_tensors(p, sigma_0, omega);
        for (i, lam_row) in lam.iter().enumerate() {
            for j in 0..3 {
                let mut prod = c64::new(0.0, 0.0);
                for (k, lam_ik) in lam_row.iter().enumerate() {
                    prod += *lam_ik * lam_inv[k][j];
                }
                let want = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (prod - c64::new(want, 0.0)).norm() < 1e-13,
                    "(Λ·Λ⁻¹)[{i}][{j}] = {prod:?}"
                );
            }
        }
        // Λ r̂ = (1/s) r̂.
        let u = ((r - crate::mesh::R_PML_INNER)
            / (crate::mesh::R_BUFFER - crate::mesh::R_PML_INNER))
            .clamp(0.0, 1.0);
        let s = c64::new(1.0, -sigma_0 * u * u / omega);
        let s_inv = c64::new(1.0, 0.0) / s;
        let r_hat = p.map(|x| x / r);
        for i in 0..3 {
            let mut got = c64::new(0.0, 0.0);
            for k in 0..3 {
                got += lam[i][k] * c64::new(r_hat[k], 0.0);
            }
            let want = s_inv * c64::new(r_hat[i], 0.0);
            assert!(
                (got - want).norm() < 1e-13,
                "Λr̂[{i}] = {got:?}, want (1/s)r̂ = {want:?}"
            );
        }
        // Absorption sign: transverse eigenvalue s has Im < 0.
        assert!(s.im < 0.0);
        // σ₀ = 0: identity even in the shell.
        let (lam0, lam_inv0) = upml_matched_tensors(p, 0.0, omega);
        for i in 0..3 {
            for j in 0..3 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((lam0[i][j] - c64::new(want, 0.0)).norm() < 1e-14);
                assert!((lam_inv0[i][j] - c64::new(want, 0.0)).norm() < 1e-14);
            }
        }
    }

    /// The plane wave has unit amplitude and the correct phase
    /// convention (`exp(−iωz)` for `+z` propagation under `exp(+jωt)`).
    #[test]
    fn plane_wave_phase_convention() {
        let omega = 2.0;
        let e = plane_wave_e_inc(omega, [0.5, -0.3, 0.25]);
        // exp(−i·2·0.25) = exp(−i·0.5).
        assert!((e[0].re - 0.5_f64.cos()).abs() < 1e-15);
        assert!((e[0].im + 0.5_f64.sin()).abs() < 1e-15);
        assert_eq!(e[1], c64::new(0.0, 0.0));
        assert_eq!(e[2], c64::new(0.0, 0.0));
        assert!((e[0].norm() - 1.0).abs() < 1e-15);
    }
}
