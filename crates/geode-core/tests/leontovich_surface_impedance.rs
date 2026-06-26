//! Validation of the Leontovich surface-impedance BC for thick
//! conductors (Epic #193, issue #204).
//!
//! The Leontovich BC replaces a thick-conductor interior with
//! `E_t = −Z_s(ω) n̂ × H` on the conductor surface, which in the weak
//! form is the complex-scaled surface mass `+(iω/Z_s(ω)) S_Γ` — the
//! Silver-Müller term with `1/η₀ → 1/Z_s(ω)`. These tests pin down:
//!
//! 1. **Complex symmetry** — `A(ω)ᵀ = A(ω)` with the Leontovich term
//!    active (PR #55 invariant: the pencil is complex-symmetric, never
//!    Hermitian).
//! 2. **Silver-Müller limit** — `Z_s = η₀` (`Fixed(1)` natural units)
//!    reproduces the first-order Silver-Müller system: the solver's
//!    solution satisfies an *independently composed* dense
//!    `A = K + ik₀S − ω²M` to direct-solve residual precision.
//! 3. **PEC limit** — `Z_s → 0` approaches the PEC solution on the
//!    same geometry (wall edges eliminated), monotonically in `Z_s`.
//! 4. **Volumetric-σ oracle** — on the 1D conducting-slab fixture from
//!    the skin-effect test (#196), replacing the meshed conductor
//!    half `x > 1/2` of the unit cube with a Leontovich wall at
//!    `x = 1/2` reproduces (a) the dissipated (surface-loss) power of
//!    the volumetric-σ reference and (b) the vacuum-side field,
//!    within mesh-convergence tolerance.
//! 5. **√ω skin-loss scaling** — across ≥3 frequencies at fixed σ, the
//!    effective surface resistance `R_eff = 2P_loss / ∮|H_t|² dS`
//!    (with `H_t` measured *independently* from the discrete curl of
//!    the FEM solution, not from the BC relation) matches the analytic
//!    `R_s(ω) = √(ωμ/2σ)` level and its `√ω` power law.
//!
//! # The slab fixture (shared with `sigma_conductivity.rs`)
//!
//! Domain `[0,1]³`, PEC walls, source `J = ẑ sin(πy)` in the first
//! element layer `x < h`, driven at **ω = π** where the transverse
//! cutoff cancels the displacement term (`κ² = π² − ω² + iωσ = iωσ`).
//! At that frequency the semi-infinite conductor's modal input
//! impedance is *exactly* the Leontovich good-conductor value
//! `−u′/u = (1+i)/δ = iω/Z_s`, so the volumetric and surface-impedance
//! formulations agree up to (i) discretization of the skin decay in
//! the volumetric run (2 elements per δ at n = 8) and (ii) the finite
//! conductor thickness (2δ → `e^{−4}` ≈ 2% residual reflection off the
//! backing PEC wall).

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;
use std::collections::BTreeMap;
use std::f64::consts::PI;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, assemble_nedelec_current_rhs,
    assemble_nedelec_sigma_damping, cube_pec_interior_edges,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::assembly::surface::{assemble_silver_muller_surface, assemble_surface_mass};
use geode_core::backend::DefaultBackend;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
    driven_solve, driven_solve_with_sigma, driven_solve_with_surface_impedance,
};
use geode_core::mesh::TET_LOCAL_EDGES;
use geode_core::mesh::{TetMesh, cube_tet_mesh};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn readback_f64<const D: usize>(t: Tensor<B, D>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Per-tet edge index/sign tables in the form the assemblers take.
fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let tet_edges = mesh.tet_edges();
    let idx = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let sign = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    (idx, sign)
}

const GEOM_TOL: f64 = 1e-9;

/// The unit cube truncated to the vacuum half `x ≤ x_cut`, with the
/// truncation wall exposed as a list of conforming surface triangles.
///
/// Node coordinates (and hence node indices) are shared with the full
/// `cube_tet_mesh(n, 1.0)`, so centerline edges can be compared between
/// the truncated and full meshes by node-pair key.
struct TruncatedSlab {
    mesh: TetMesh,
    /// Triangles on the `x = x_cut` wall.
    wall_tris: Vec<[u32; 3]>,
    /// Index (into `mesh.tets`) of the tet owning each wall triangle.
    wall_tet: Vec<usize>,
    /// PEC mask: side/back walls eliminated, `x = x_cut` wall KEPT
    /// (impedance surface).
    interior_mask: Vec<bool>,
    /// PEC mask with the `x = x_cut` wall eliminated too (full PEC box).
    interior_mask_pec_wall: Vec<bool>,
}

fn truncated_slab(n: usize, x_cut: f64) -> TruncatedSlab {
    let full = cube_tet_mesh(n, 1.0);
    let tets: Vec<[u32; 4]> = full
        .tets
        .iter()
        .filter(|t| {
            t.iter()
                .all(|&v| full.nodes[v as usize][0] <= x_cut + GEOM_TOL)
        })
        .copied()
        .collect();
    assert!(!tets.is_empty(), "truncation removed every tet");
    let mesh = TetMesh {
        nodes: full.nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };

    // Wall triangles: tet faces whose three vertices all sit on x = x_cut.
    let mut wall_tris = Vec::new();
    let mut wall_tet = Vec::new();
    for (ti, tet) in mesh.tets.iter().enumerate() {
        for omit in 0..4 {
            let tri: [u32; 3] = {
                let mut it = (0..4).filter(|&v| v != omit).map(|v| tet[v]);
                std::array::from_fn(|_| it.next().unwrap())
            };
            if tri
                .iter()
                .all(|&v| (mesh.nodes[v as usize][0] - x_cut).abs() < GEOM_TOL)
            {
                wall_tris.push(tri);
                wall_tet.push(ti);
            }
        }
    }
    assert_eq!(
        wall_tris.len(),
        2 * n * n,
        "expected 2 wall triangles per hex face"
    );

    let edges = mesh.edges();
    let on = |v: u32, k: usize, val: f64| (mesh.nodes[v as usize][k] - val).abs() < GEOM_TOL;
    let on_pec_side = |e: &[u32; 2]| {
        let planes = [(0usize, 0.0), (1, 0.0), (1, 1.0), (2, 0.0), (2, 1.0)];
        planes
            .iter()
            .any(|&(k, val)| on(e[0], k, val) && on(e[1], k, val))
    };
    let interior_mask: Vec<bool> = edges.iter().map(|e| !on_pec_side(e)).collect();
    let interior_mask_pec_wall: Vec<bool> = edges
        .iter()
        .zip(interior_mask.iter())
        .map(|(e, &keep)| keep && !(on(e[0], 0, x_cut) && on(e[1], 0, x_cut)))
        .collect();

    TruncatedSlab {
        mesh,
        wall_tris,
        wall_tet,
        interior_mask,
        interior_mask_pec_wall,
    }
}

/// The slab-fixture source `J = ẑ sin(πy)` on the first element layer
/// `x < h` (identical closure for the full and truncated meshes — the
/// truncated mesh simply has no tets beyond `x_cut`).
fn slab_source(mesh: &TetMesh, h: f64) -> CurrentSource {
    CurrentSource::from_centroids(mesh, |c| {
        let jz = if c[0] < h { (PI * c[1]).sin() } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    })
}

/// `e^H S e` for a real symmetric `S` given as a flat row-major
/// `[n × n]` slice (the imaginary part cancels by symmetry).
fn quad_form_real_sym(s_flat: &[f64], e: &[c64]) -> f64 {
    let n = e.len();
    assert_eq!(s_flat.len(), n * n);
    let mut acc = 0.0_f64;
    for i in 0..n {
        let row = &s_flat[i * n..(i + 1) * n];
        let (a_i, b_i) = (e[i].re, e[i].im);
        for (j, &s_ij) in row.iter().enumerate() {
            if s_ij != 0.0 {
                acc += s_ij * (a_i * e[j].re + b_i * e[j].im);
            }
        }
    }
    acc
}

/// `e^H S e` for a faer dense matrix.
fn quad_form_faer(s: &faer::Mat<f64>, e: &[c64]) -> f64 {
    let n = e.len();
    let mut acc = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            let s_ij = s[(i, j)];
            if s_ij != 0.0 {
                acc += s_ij * (e[i].re * e[j].re + e[i].im * e[j].im);
            }
        }
    }
    acc
}

/// Time-averaged power absorbed by a Leontovich wall:
/// `P = ½ Re(1/Z̄_s) ∮ |E_t|² dS = ½ Re(1/Z̄_s) eᴴ S_Γ e`.
fn leontovich_loss_power(
    s_wall: &faer::Mat<f64>,
    e: &[c64],
    model: SurfaceImpedanceModel,
    omega: f64,
) -> f64 {
    let z = model.z_s(omega);
    let re_inv_zbar = z.re / (z.re * z.re + z.im * z.im);
    0.5 * re_inv_zbar * quad_form_faer(s_wall, e)
}

/// Barycentric gradients `∇λ_0..3` of a tet.
fn tet_grad_lambda(p: &[[f64; 3]; 4]) -> [[f64; 3]; 4] {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let e1 = sub(p[1], p[0]);
    let e2 = sub(p[2], p[0]);
    let e3 = sub(p[3], p[0]);
    let v6 = dot(e1, cross(e2, e3)); // 6 × signed volume
    let g1 = cross(e2, e3).map(|x| x / v6);
    let g2 = cross(e3, e1).map(|x| x / v6);
    let g3 = cross(e1, e2).map(|x| x / v6);
    let g0 = [
        -(g1[0] + g2[0] + g3[0]),
        -(g1[1] + g2[1] + g3[1]),
        -(g1[2] + g2[2] + g3[2]),
    ];
    [g0, g1, g2, g3]
}

/// Element-constant `∇×E` of the Whitney edge field on tet `ti`.
///
/// `∇×(λ_a ∇λ_b − λ_b ∇λ_a) = 2 ∇λ_a × ∇λ_b`, scattered with the same
/// local-edge order and global-orientation signs as the assembly
/// ([`TET_LOCAL_EDGES`], `TetMesh::tet_edges`).
fn tet_curl_e(mesh: &TetMesh, tet_edge_rows: &[[(u32, i8); 6]], ti: usize, e: &[c64]) -> [c64; 3] {
    let tet = mesh.tets[ti];
    let p: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
    let g = tet_grad_lambda(&p);
    let mut curl = [c64::new(0.0, 0.0); 3];
    for (k, &(la, lb)) in TET_LOCAL_EDGES.iter().enumerate() {
        let (gidx, sign) = tet_edge_rows[ti][k];
        let d = e[gidx as usize] * (sign as f64);
        let (a, b) = (g[la], g[lb]);
        let cab = [
            2.0 * (a[1] * b[2] - a[2] * b[1]),
            2.0 * (a[2] * b[0] - a[0] * b[2]),
            2.0 * (a[0] * b[1] - a[1] * b[0]),
        ];
        for x in 0..3 {
            curl[x] += d * cab[x];
        }
    }
    curl
}

fn triangle_area(p: &[[f64; 3]; 3]) -> f64 {
    let u = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let v = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let c = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

/// `∮ |H_t|² dS` over the `x = x_cut` wall, with `H = ∇×E / (−iω)`
/// measured from the element-constant curl of the tet adjacent to each
/// wall triangle (independent of the BC relation `E_t = −Z_s n̂×H`).
/// Tangential components on the wall (normal `x̂`) are `H_y, H_z`.
fn wall_ht_squared(slab: &TruncatedSlab, e: &[c64], omega: f64) -> f64 {
    let tet_edge_rows = slab.mesh.tet_edges();
    let mut acc = 0.0_f64;
    for (tri, &ti) in slab.wall_tris.iter().zip(slab.wall_tet.iter()) {
        let curl = tet_curl_e(&slab.mesh, &tet_edge_rows, ti, e);
        // |H_t|² = (|curl_y|² + |curl_z|²) / ω².
        let ht2 = (curl[1].re * curl[1].re
            + curl[1].im * curl[1].im
            + curl[2].re * curl[2].re
            + curl[2].im * curl[2].im)
            / (omega * omega);
        let p: [[f64; 3]; 3] = std::array::from_fn(|v| slab.mesh.nodes[tri[v] as usize]);
        acc += triangle_area(&p) * ht2;
    }
    acc
}

/// `|E_z|` on the z-directed centerline edge at `(x = i·h, y = 1/2,
/// z = 1/2 → 1/2 + h)`, looked up by node-pair key (valid for both the
/// full and truncated meshes, which share node numbering).
fn centerline_ez(mesh: &TetMesh, e: &[c64], n: usize, i: usize) -> f64 {
    let nps = n + 1;
    let node_idx = |i: usize, j: usize, k: usize| -> u32 { (i + j * nps + k * nps * nps) as u32 };
    let a = node_idx(i, n / 2, n / 2);
    let b = node_idx(i, n / 2, n / 2 + 1);
    let key = if a < b { [a, b] } else { [b, a] };
    let idx = mesh
        .edges()
        .iter()
        .position(|edge| *edge == key)
        .expect("centerline z-edge must exist");
    e[idx].re.hypot(e[idx].im)
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

fn max_asymmetry(flat: &[f64], n: usize) -> f64 {
    let mut worst = 0.0_f64;
    for i in 0..n {
        for j in (i + 1)..n {
            worst = worst.max((flat[i * n + j] - flat[j * n + i]).abs());
        }
    }
    worst
}

// ---------------------------------------------------------------------------
// 1. Complex-symmetry regression with the Leontovich term active.
// ---------------------------------------------------------------------------

/// `A(ω) = K + iωC − ω²M + (iω/Z_s)S_Γ` must stay complex-symmetric
/// (`A(ω)ᵀ = A(ω)`) with the Leontovich term active — the coefficient
/// is a scalar complex weight on a real-symmetric surface mass, so the
/// PR #55 invariant survives.
#[test]
fn complex_symmetry_preserved_with_leontovich_term() {
    let slab = truncated_slab(4, 0.5);
    let mesh = &slab.mesh;
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &dev);

    let omega = 1.9;
    // Lossy dielectric ε + volumetric σ + Leontovich wall, all at once.
    let eps: Vec<c64> = (0..mesh.n_tets())
        .map(|e| c64::new(1.5 + 0.1 * (e % 2) as f64, -0.03))
        .collect();
    let sigma: Vec<f64> = (0..mesh.n_tets()).map(|t| 0.2 + 0.01 * t as f64).collect();
    let model = SurfaceImpedanceModel::GoodConductor { sigma: 25.0 };
    let coeff = model.weak_coefficient(omega).expect("finite Z_s");

    let sys = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps,
    );
    let c =
        assemble_nedelec_sigma_damping::<B>(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &sigma);
    let s_wall = assemble_surface_mass(mesh, &slab.wall_tris, &mesh.edges());

    let k = readback_f64(sys.k);
    let m_re = readback_f64(sys.m_re);
    let m_im = readback_f64(sys.m_im);
    let c = readback_f64(c);

    let omega2 = omega * omega;
    let mut a_re = vec![0.0_f64; n_edges * n_edges];
    let mut a_im = vec![0.0_f64; n_edges * n_edges];
    for i in 0..n_edges {
        for j in 0..n_edges {
            let idx = i * n_edges + j;
            let s_ij = s_wall[(i, j)];
            a_re[idx] = k[idx] - omega2 * m_re[idx] + coeff.re * s_ij;
            a_im[idx] = omega * c[idx] - omega2 * m_im[idx] + coeff.im * s_ij;
        }
    }

    let scale = a_re
        .iter()
        .chain(a_im.iter())
        .fold(0.0_f64, |a, &v| a.max(v.abs()));
    assert!(scale > 0.0);
    let tol = 1e-5 * scale;
    assert!(
        max_asymmetry(&a_re, n_edges) < tol,
        "Re(A(ω)) not symmetric with Leontovich term"
    );
    assert!(
        max_asymmetry(&a_im, n_edges) < tol,
        "Im(A(ω)) not symmetric with Leontovich term"
    );

    // The surface term itself must be present (nonzero) for the
    // regression to mean anything.
    let s_max = (0..n_edges)
        .flat_map(|i| (0..n_edges).map(move |j| (i, j)))
        .fold(0.0_f64, |a, (i, j)| a.max(s_wall[(i, j)].abs()));
    assert!(s_max > 0.0, "wall surface mass is identically zero");
}

// ---------------------------------------------------------------------------
// 2. Silver-Müller limit: Z_s = η₀ reproduces the SM system.
// ---------------------------------------------------------------------------

/// With `Fixed(η₀ = 1)` the solver must produce the solution of the
/// independently composed first-order Silver-Müller system
/// `A = K + ik₀S − ω²M` (with `S` from the original tagged
/// Silver-Müller assembler): the relative residual `‖Ax − b‖/‖b‖` of
/// the solver's `x` against that dense `A` must sit at the direct-solve
/// round-off floor.
#[test]
fn fixed_eta0_recovers_silver_muller_system() {
    let n = 4;
    let slab = truncated_slab(n, 0.5);
    let mesh = &slab.mesh;
    let edges = mesh.edges();
    let n_edges = edges.len();
    let (tet_idx, tet_sign) = edge_tables(mesh);
    let dev = device();

    let omega = 1.3;
    let eps = vacuum(mesh);
    let source = slab_source(mesh, 0.25);

    let sol = driven_solve_with_surface_impedance::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &slab.interior_mask,
        },
        &[SurfaceImpedanceBc {
            triangles: &slab.wall_tris,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        }],
        omega,
        &source,
        &device(),
    )
    .expect("Z_s = η₀ solve");
    assert!(sol.residual_rel < 1e-10);

    // Independent dense composition of the Silver-Müller system.
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &dev);
    let sys = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps,
    );
    let k = readback_f64(sys.k);
    let m_re = readback_f64(sys.m_re);
    // Vacuum ε is real: Im(M) ≡ 0.
    let tags = vec![7_i32; slab.wall_tris.len()];
    let s_sm = assemble_silver_muller_surface(mesh, &slab.wall_tris, &tags, 7, &edges);

    let j_re: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].re, j[1].re, j[2].re])
        .collect();
    let rhs =
        assemble_nedelec_current_rhs::<B>(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_re);
    let rhs = readback_f64(rhs);

    // r = A x − b on the kept (interior) edges; PEC rows/columns of x
    // are exact zeros so summing over all j is equivalent to the
    // reduced system.
    let omega2 = omega * omega;
    let mut res2 = 0.0_f64;
    let mut b2 = 0.0_f64;
    for i in 0..n_edges {
        if !slab.interior_mask[i] {
            continue;
        }
        let mut ax = c64::new(0.0, 0.0);
        for j in 0..n_edges {
            if !slab.interior_mask[j] {
                continue;
            }
            let a_ij = c64::new(
                k[i * n_edges + j] - omega2 * m_re[i * n_edges + j],
                omega * s_sm[(i, j)],
            );
            ax += a_ij * sol.e_edges[j];
        }
        // b = iω ∫ N_i · J dV (J real here).
        let b_i = c64::new(0.0, omega * rhs[i]);
        let r = ax - b_i;
        res2 += r.re * r.re + r.im * r.im;
        b2 += b_i.re * b_i.re + b_i.im * b_i.im;
    }
    assert!(b2 > 0.0, "source must produce a nonzero RHS");
    let rel = (res2 / b2).sqrt();
    eprintln!("Silver-Müller-limit residual against independent dense A: {rel:.3e}");
    assert!(
        rel < 1e-8,
        "Fixed(η₀) solution does not satisfy the Silver-Müller system: rel residual {rel:.3e}"
    );
}

// ---------------------------------------------------------------------------
// 3. PEC limit: Z_s → 0 approaches the PEC solution.
// ---------------------------------------------------------------------------

/// As `Z_s → 0` the impedance wall must converge to the PEC solution
/// on the same geometry (wall edges eliminated): the relative field
/// difference shrinks with `Z_s` and is small at `Z_s = 10⁻⁵ η₀`.
#[test]
fn small_z_s_recovers_pec_wall() {
    let n = 4;
    let slab = truncated_slab(n, 0.5);
    let mesh = &slab.mesh;
    let eps = vacuum(mesh);
    let source = slab_source(mesh, 0.25);
    let omega = 1.0;

    let pec = driven_solve::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &slab.interior_mask_pec_wall,
        },
        omega,
        &source,
        &device(),
    )
    .expect("PEC reference solve");
    let norm: f64 = pec
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);

    let rel_diff = |z: f64| -> f64 {
        let sol = driven_solve_with_surface_impedance::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &slab.wall_tris,
                model: SurfaceImpedanceModel::Fixed(c64::new(z, 0.0)),
            }],
            omega,
            &source,
            &device(),
        )
        .expect("small-Z_s solve");
        let d2: f64 = sol
            .e_edges
            .iter()
            .zip(pec.e_edges.iter())
            .map(|(a, b)| {
                let d = *a - *b;
                d.re * d.re + d.im * d.im
            })
            .sum();
        d2.sqrt() / norm
    };

    let d_coarse = rel_diff(1e-2);
    let d_fine = rel_diff(1e-5);
    eprintln!("PEC-limit relative differences: Z_s = 1e-2 → {d_coarse:.3e}, 1e-5 → {d_fine:.3e}");
    assert!(
        d_fine < d_coarse,
        "field difference to PEC must shrink with Z_s: {d_fine:.3e} !< {d_coarse:.3e}"
    );
    assert!(
        d_fine < 1e-3,
        "Z_s = 1e-5 η₀ should be PEC to ~1e-3: relative diff {d_fine:.3e}"
    );
}

// ---------------------------------------------------------------------------
// 4. Volumetric-σ oracle (the affordable geometry from #196).
// ---------------------------------------------------------------------------

/// On the 1D conducting-slab fixture at ω = π (where the Leontovich
/// good-conductor impedance is the *exact* modal input impedance of
/// the semi-infinite conductor), replacing the meshed conductor half
/// with the Leontovich wall must reproduce
///
/// (a) the dissipated power — volumetric `½σ∫|E|²dV` vs surface
///     `½Re(1/Z̄_s)∮|E_t|²dS` — and
/// (b) the vacuum-side centerline field
///
/// within mesh-convergence tolerance (the volumetric run resolves the
/// skin decay with only 2 elements per δ at n = 8, and the conductor
/// is 2δ thick → ~2% residual reflection).
#[test]
fn leontovich_loss_matches_volumetric_sigma_oracle() {
    let n = 8;
    let h = 1.0 / n as f64;
    let omega = PI;
    let delta = 0.25;
    let sigma = 2.0 / (omega * delta * delta); // δ = √(2/ωσ) = 0.25

    // --- Volumetric-σ reference: full cube, conductor in x > 1/2 ---------
    let full = cube_tet_mesh(n, 1.0);
    let (_, full_interior) = cube_pec_interior_edges(&full, 1.0);
    let eps_full = vacuum(&full);
    let sigma_tet: Vec<f64> = geode_core::assembly::nedelec::tet_centroids(&full)
        .iter()
        .map(|c| if c[0] > 0.5 { sigma } else { 0.0 })
        .collect();
    let source_full = slab_source(&full, h);
    let sol_vol = driven_solve_with_sigma::<B>(
        &full,
        DrivenMaterials::Scalar(&eps_full),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &full_interior,
        },
        omega,
        &source_full,
        &device(),
    )
    .expect("volumetric-σ solve");
    assert!(sol_vol.residual_rel < 1e-8);

    // P_vol = ½ ∫ σ|E|² dV = ½ eᴴ C(σ) e.
    let n_edges_full = full.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&full);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&full, &dev);
    let c_mat = assemble_nedelec_sigma_damping::<B>(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges_full,
        &sigma_tet,
    );
    let c_flat = readback_f64(c_mat);
    let p_vol = 0.5 * quad_form_real_sym(&c_flat, &sol_vol.e_edges);
    assert!(p_vol > 0.0, "volumetric loss must be positive");

    // --- Leontovich: vacuum half-domain, impedance wall at x = 1/2 -------
    let slab = truncated_slab(n, 0.5);
    let eps_slab = vacuum(&slab.mesh);
    let source_slab = slab_source(&slab.mesh, h);
    let model = SurfaceImpedanceModel::GoodConductor { sigma };
    let sol_leo = driven_solve_with_surface_impedance::<B>(
        &slab.mesh,
        DrivenMaterials::Scalar(&eps_slab),
        None,
        &DrivenBcs {
            pec_interior_mask: &slab.interior_mask,
        },
        &[SurfaceImpedanceBc {
            triangles: &slab.wall_tris,
            model,
        }],
        omega,
        &source_slab,
        &device(),
    )
    .expect("Leontovich solve");
    assert!(sol_leo.residual_rel < 1e-8);

    let s_wall = assemble_surface_mass(&slab.mesh, &slab.wall_tris, &slab.mesh.edges());
    let p_leo = leontovich_loss_power(&s_wall, &sol_leo.e_edges, model, omega);
    assert!(p_leo > 0.0, "Leontovich surface loss must be positive");

    // (a) Surface-loss agreement within mesh-convergence tolerance.
    let rel_p = (p_leo - p_vol).abs() / p_vol;
    eprintln!(
        "loss power: volumetric σ = {p_vol:.6e}, Leontovich = {p_leo:.6e} \
         (relative diff {:.2}%)",
        100.0 * rel_p
    );
    assert!(
        rel_p < 0.25,
        "Leontovich loss {p_leo:.4e} vs volumetric {p_vol:.4e}: \
         relative diff {rel_p:.3} above mesh tolerance"
    );

    // (b) Vacuum-side centerline field agreement (x = 1/4, 3/8 — between
    // the source layer and the wall).
    for i in [n / 4, 3 * n / 8] {
        let ez_vol = centerline_ez(&full, &sol_vol.e_edges, n, i);
        let ez_leo = centerline_ez(&slab.mesh, &sol_leo.e_edges, n, i);
        let rel = (ez_leo - ez_vol).abs() / ez_vol;
        eprintln!(
            "centerline |E_z| at x = {:.3}: volumetric {ez_vol:.6e}, Leontovich {ez_leo:.6e} \
             (relative diff {:.2}%)",
            i as f64 * h,
            100.0 * rel
        );
        assert!(
            rel < 0.15,
            "vacuum-side field mismatch at x = {}: {rel:.3}",
            i as f64 * h
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Analytic √ω skin-loss scaling across ≥ 3 frequencies.
// ---------------------------------------------------------------------------

fn cdiv(a: c64, b: c64) -> c64 {
    let d = b.re * b.re + b.im * b.im;
    c64::new(
        (a.re * b.re + a.im * b.im) / d,
        (a.im * b.re - a.re * b.im) / d,
    )
}

fn ccosh(z: c64) -> c64 {
    c64::new(z.re.cosh() * z.im.cos(), z.re.sinh() * z.im.sin())
}

fn csinh(z: c64) -> c64 {
    c64::new(z.re.sinh() * z.im.cos(), z.re.cosh() * z.im.sin())
}

/// Closed-form modal solution of the half-domain slab fixture at the
/// impedance wall.
///
/// With `E = ẑ u(x) sin(πy)`, the fixture reduces to the two-region
/// modal BVP on `[0, L]`:
///
/// ```text
/// −u″ + κ₀² u = iω · 1_{x<h},   κ₀² = π² − ω²,
/// u(0) = 0,                      (PEC source-side wall)
/// u′(L) = −γ u(L),               γ = iω/Z_s = (1+i)√(ωσ/2),
/// ```
///
/// solved exactly with cosh/sinh fundamental solutions. Returns
/// `u(L)`. Requires `ω ≠ π` (κ₀ ≠ 0).
fn modal_wall_field(omega: f64, sigma: f64, h: f64, l: f64) -> c64 {
    let k2 = PI * PI - omega * omega;
    assert!(k2.abs() > 1e-9, "modal reference needs κ₀ ≠ 0 (ω ≠ π)");
    let kappa = if k2 >= 0.0 {
        c64::new(k2.sqrt(), 0.0)
    } else {
        c64::new(0.0, (-k2).sqrt())
    };
    let g = (omega * sigma / 2.0).sqrt();
    let gamma = c64::new(g, g);
    // Particular solution in the source layer: u_p = iω/κ₀².
    let up = cdiv(c64::new(0.0, omega), c64::new(k2, 0.0));

    // u₁ = u_p − u_p cosh(κx) + D sinh(κx) on [0, h] (u₁(0) = 0),
    // u₂ = A [cosh(κ(x−L)) − (γ/κ) sinh(κ(x−L))] on [h, L]
    //      (u₂′(L) = −γ u₂(L) built in). Match u, u′ at x = h.
    let ch1 = ccosh(kappa * h);
    let sh1 = csinh(kappa * h);
    let m = kappa * (h - l);
    let ch2 = ccosh(m);
    let sh2 = csinh(m);
    let gok = cdiv(gamma, kappa);
    let p = ch2 - gok * sh2;
    let q = sh2 - gok * ch2;
    // [ sh1  −p ] [D]   [ u_p (ch1 − 1) ]
    // [ ch1  −q ] [A] = [ u_p sh1       ]
    let det = p * ch1 - q * sh1;
    let r1 = up * (ch1 - c64::new(1.0, 0.0));
    let r2 = up * sh1;
    // u(L) = A.
    cdiv(sh1 * r2 - ch1 * r1, det)
}

/// At fixed σ the skin-loss power must follow the analytic
/// `R_s(ω) = √(ω/2σ)` ∝ `√ω` law across ≥3 frequencies. Two
/// independent extractions:
///
/// 1. **Analytic modal reference** — the FEM surface loss `P_fem(ω)`
///    must match the closed-form `P(ω) = ¼ Re(1/Z̄_s) |u(L)|²` of the
///    modal BVP per frequency, and the analytically-normalized surface
///    resistance `R(ω) = 2 P_fem ω² / (½ |γ u(L)|²)` (i.e. `P_fem`
///    divided by the analytic `∮|H_t|² dS`) must fit the `√ω` power
///    law.
/// 2. **Discrete-curl level check** — `R_eff = 2 P_fem / ∮|H_t,fem|²`
///    with `H_t` measured from the element-constant curl of the wall
///    tets (independent of the BC relation) must sit at the analytic
///    `R_s(ω)` level (this estimator carries an O(h) wall-offset bias,
///    so it pins the level, not the exponent).
#[test]
fn leontovich_loss_follows_sqrt_omega_scaling() {
    let n = 8;
    let h = 1.0 / n as f64;
    let l = 0.5;
    let sigma = 32.0 / PI; // same conductor as the oracle test
    let slab = truncated_slab(n, l);
    let eps = vacuum(&slab.mesh);
    let source = slab_source(&slab.mesh, h);
    let s_wall = assemble_surface_mass(&slab.mesh, &slab.wall_tris, &slab.mesh.edges());
    let model = SurfaceImpedanceModel::GoodConductor { sigma };

    // ω ≠ π so the modal reference's κ₀ ≠ 0; spans a 2.25× frequency
    // range (1.5× in √ω).
    let omegas = [2.0, 3.0, 4.5];
    let mut r_norm = Vec::new();
    for &omega in omegas.iter() {
        let sol = driven_solve_with_surface_impedance::<B>(
            &slab.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &slab.wall_tris,
                model,
            }],
            omega,
            &source,
            &device(),
        )
        .expect("scaling-sweep solve");
        assert!(sol.residual_rel < 1e-8);

        let p_fem = leontovich_loss_power(&s_wall, &sol.e_edges, model, omega);
        assert!(p_fem > 0.0);
        let r_s = (omega / (2.0 * sigma)).sqrt();

        // (1) Analytic modal reference.
        let u_l = modal_wall_field(omega, sigma, h, l);
        let u_l2 = u_l.re * u_l.re + u_l.im * u_l.im;
        let z = model.z_s(omega);
        let re_inv_zbar = z.re / (z.re * z.re + z.im * z.im);
        // ∮|E_t|² dS = |u(L)|²/2 (∫ sin²(πy) dy = 1/2), so
        // P = ¼ Re(1/Z̄_s)|u(L)|²; ∮|H_t|² dS = ½|γ u(L)|²/ω².
        let p_analytic = 0.25 * re_inv_zbar * u_l2;
        let gamma2 = omega * sigma; // |γ|² = 2·(ωσ/2)
        let ht2_analytic = 0.5 * gamma2 * u_l2 / (omega * omega);
        let rel_p = (p_fem - p_analytic).abs() / p_analytic;
        let r = 2.0 * p_fem / ht2_analytic;
        eprintln!(
            "ω = {omega:.4}: P_fem = {p_fem:.4e}, P_analytic = {p_analytic:.4e} \
             (diff {:.2}%), R = {r:.5} vs R_s = {r_s:.5}",
            100.0 * rel_p
        );
        assert!(
            rel_p < 0.12,
            "FEM surface loss at ω = {omega} strays from the analytic modal \
             prediction: relative diff {rel_p:.3}"
        );
        r_norm.push(r);

        // (2) Discrete-curl surface-resistance level.
        let ht2_fem = wall_ht_squared(&slab, &sol.e_edges, omega);
        assert!(ht2_fem > 0.0);
        let r_eff = 2.0 * p_fem / ht2_fem;
        eprintln!(
            "          discrete-curl R_eff = {r_eff:.5} (ratio to R_s: {:.4})",
            r_eff / r_s
        );
        assert!(
            (r_eff / r_s - 1.0).abs() < 0.2,
            "effective surface resistance at ω = {omega}: \
             R_eff = {r_eff:.4} vs R_s = {r_s:.4}"
        );
    }

    // Power-law exponent of the analytically-normalized surface
    // resistance across the frequency span (√ω → 0.5).
    let p_fit = (r_norm[2] / r_norm[0]).ln() / (omegas[2] / omegas[0]).ln();
    eprintln!("fitted R(ω) power-law exponent: {p_fit:.4} (analytic 0.5)");
    assert!(
        (p_fit - 0.5).abs() < 0.1,
        "skin-loss scaling exponent {p_fit:.3} strays from √ω"
    );

    // Monotone increase is implied by the power law but cheap to pin.
    assert!(r_norm[0] < r_norm[1] && r_norm[1] < r_norm[2]);
}

// ---------------------------------------------------------------------------
// 6. No-op composition: empty surface list reproduces driven_solve.
// ---------------------------------------------------------------------------

/// An empty surface list must reproduce `driven_solve` exactly
/// (bit-for-bit at the linear-system level — same assembly, same
/// factorization). This pins the no-op default now that the shared
/// `driven_solve_impl` also threads the lumped-port surface terms
/// (issue #202): with `ports = &[]` and `surfaces = &[]` the system is
/// the plain `A(ω) = K + iωC − ω²M`.
#[test]
fn empty_surface_list_matches_driven_solve() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, mask) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((PI * c[2]).sin(), 0.0),
            c64::new(0.0, 0.0),
        ]
    });
    let omega = 1.1;
    let sol_s = driven_solve_with_surface_impedance::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        omega,
        &source,
        &device(),
    )
    .expect("empty-surfaces solve");
    let sol_0 = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("plain solve");
    assert_eq!(sol_s.n_interior, sol_0.n_interior);
    for (a, b) in sol_s.e_edges.iter().zip(sol_0.e_edges.iter()) {
        assert_eq!(a, b, "empty surface list changed the solution");
    }
}
