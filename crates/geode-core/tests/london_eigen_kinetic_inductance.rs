//! Eigenmode **kinetic-inductance frequency shift** of London
//! superconducting walls vs first-order perturbation theory (Epic #475,
//! issue #604).
//!
//! A cavity wall with the London surface impedance `Z_s = iωμλ_L` shifts
//! the resonance below the PEC value by, to first order in `λ_L`,
//!
//! ```text
//! Δω/ω ≈ −(λ_L/2) · ∮|H_t|² dA / ∫ μ|H|² dV,
//! ```
//!
//! the standard wall-recession / kinetic-inductance result (frequency
//! **decreases** as the penetration depth grows). On the real symmetric
//! eigen pencil the London wall is the K-side stiffness `λ_L⁻¹ S_Γ`
//! ([`geode_core::eigen::transmon::LondonSurface`]) — a penalty whose
//! `λ_L → 0` limit is the PEC wall.
//!
//! **Fixture:** a rectangular `1.0 × 0.85 × 0.7` vacuum box (distinct
//! side lengths so the lowest mode is simple — no cube degeneracy),
//! London on all six walls. The PEC baseline is solved on the same mesh
//! with the wall edges eliminated; `H_t` for the perturbation prediction
//! is measured **independently** from the element-constant discrete curl
//! of the PEC eigenvector (the same convention as the `R_eff` extraction
//! in `leontovich_surface_impedance.rs` test 5), and `∫|H|² dV` from the
//! same discrete curl over the volume, so the two integrals share their
//! discretization bias.
//!
//! **What is pinned (honest-band convention, mirroring #504):**
//!
//! 1. The FEM shift is negative (frequency drops) and **monotone** in
//!    λ_L across the decreasing sweep.
//! 2. The measured/predicted ratio sits in an honest agreement band at
//!    every sweep point, and the smallest-λ_L point agrees at least as
//!    well as the largest (convergence trend toward the perturbation
//!    prediction as λ_L → 0). The sweep stops at λ_L = 5×10⁻³ — well
//!    before the `λ_L⁻¹` penalty's conditioning noise; exact λ_L = 0 is
//!    invalid by convention (PEC goes through the edge mask).

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;
use std::collections::BTreeMap;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::lanczos::InnerSolver;
use geode_core::eigen::transmon::{
    LondonSurface, LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil,
    solve_transmon_eigenmodes_full, solve_transmon_eigenmodes_full_with_london,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TET_LOCAL_EDGES, TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

const M_PER_UNIT: f64 = 1e-6;
const GEOM_TOL: f64 = 1e-9;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// A rectangular vacuum box `dims[0] × dims[1] × dims[2]` (anisotropic
/// rescale of the unit-cube mesh so the lowest resonance is simple), with
/// every boundary triangle listed alongside its owner tet and the axis it
/// is normal to.
struct BoxFixture {
    mesh: TetMesh,
    edges: Vec<[u32; 2]>,
    tet_edge_sign: Vec<[i8; 6]>,
    scatter: NedelecScatterMap,
    /// All boundary triangles (all six walls).
    wall_tris: Vec<[u32; 3]>,
    /// Owner tet of each wall triangle.
    wall_tet: Vec<usize>,
    /// Axis (0/1/2) each wall triangle is normal to.
    wall_axis: Vec<usize>,
    /// PEC mask: every wall edge eliminated (the PEC baseline).
    interior_mask_pec: Vec<bool>,
    /// London mask: every edge kept (the wall carries the surface term).
    keep_all_mask: Vec<bool>,
}

fn box_fixture(n: usize, dims: [f64; 3]) -> BoxFixture {
    let mut mesh = cube_tet_mesh(n, 1.0);
    for p in mesh.nodes.iter_mut() {
        for k in 0..3 {
            p[k] *= dims[k];
        }
    }
    let mesh = TetMesh {
        nodes: mesh.nodes,
        tets: mesh.tets,
        physical_groups: BTreeMap::new(),
    };

    let on_plane = |v: u32, k: usize, val: f64| (mesh.nodes[v as usize][k] - val).abs() < GEOM_TOL;
    let mut wall_tris = Vec::new();
    let mut wall_tet = Vec::new();
    let mut wall_axis = Vec::new();
    for (ti, tet) in mesh.tets.iter().enumerate() {
        for omit in 0..4 {
            let tri: [u32; 3] = {
                let mut it = (0..4).filter(|&v| v != omit).map(|v| tet[v]);
                std::array::from_fn(|_| it.next().unwrap())
            };
            for (k, &dim) in dims.iter().enumerate() {
                for val in [0.0, dim] {
                    if tri.iter().all(|&v| on_plane(v, k, val)) {
                        wall_tris.push(tri);
                        wall_tet.push(ti);
                        wall_axis.push(k);
                    }
                }
            }
        }
    }
    assert_eq!(
        wall_tris.len(),
        6 * 2 * n * n,
        "expected 2n² triangles per wall"
    );

    let edges = mesh.edges();
    let interior_mask_pec = pec_interior_mask_from_triangles(&edges, &[wall_tris.as_slice()]);
    let keep_all_mask = vec![true; edges.len()];
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&mesh);
    let scatter = NedelecScatterMap::new(&tet_edge_idx);

    BoxFixture {
        mesh,
        edges,
        tet_edge_sign,
        scatter,
        wall_tris,
        wall_tet,
        wall_axis,
        interior_mask_pec,
        keep_all_mask,
    }
}

/// Real vacuum (ε = 1, μ = 1) Nédélec pencil value vectors.
fn assemble_vacuum_pencil(fx: &BoxFixture) -> (Vec<f64>, Vec<f64>) {
    let (nodes_t, tets_t) = upload_mesh::<B>(&fx.mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let tensors = vec![identity; fx.mesh.n_tets()];
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        &fx.tet_edge_sign,
        &fx.scatter,
        &tensors,
        &tensors,
    );
    (vals_to_host(sys.k_re_vals), vals_to_host(sys.m_re_vals))
}

/// No-port shunt: `L = ∞`, `C = 0` — the pencil is the pure volume
/// `(K, M)` (plus whatever London walls the entry point adds).
fn bare_shunt(faces: &[[u32; 3]]) -> LumpedReactiveShunt<'_> {
    LumpedReactiveShunt {
        faces,
        length: 1.0,
        width: 1.0,
        element: ReactiveElementNatural {
            l_natural: f64::INFINITY,
            c_natural: 0.0,
        },
    }
}

/// Scatter a reduced eigenvector back to full edge length under a mask.
fn scatter_full(mask: &[bool], reduced: &[f64]) -> Vec<f64> {
    let mut xf = vec![0.0_f64; mask.len()];
    let mut r = 0usize;
    for (e, &keep) in mask.iter().enumerate() {
        if keep {
            xf[e] = reduced[r];
            r += 1;
        }
    }
    assert_eq!(r, reduced.len());
    xf
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
    let v6 = dot(e1, cross(e2, e3));
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

/// Element-constant `∇×E` of the real Whitney edge field on tet `ti`.
fn tet_curl(mesh: &TetMesh, tet_edge_rows: &[[(u32, i8); 6]], ti: usize, xf: &[f64]) -> [f64; 3] {
    let tet = mesh.tets[ti];
    let p: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
    let g = tet_grad_lambda(&p);
    let mut curl = [0.0_f64; 3];
    for (k, &(la, lb)) in TET_LOCAL_EDGES.iter().enumerate() {
        let (gidx, sign) = tet_edge_rows[ti][k];
        let d = xf[gidx as usize] * (sign as f64);
        let (a, b) = (g[la], g[lb]);
        curl[0] += d * 2.0 * (a[1] * b[2] - a[2] * b[1]);
        curl[1] += d * 2.0 * (a[2] * b[0] - a[0] * b[2]);
        curl[2] += d * 2.0 * (a[0] * b[1] - a[1] * b[0]);
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

fn tet_volume(p: &[[f64; 3]; 4]) -> f64 {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let e1 = sub(p[1], p[0]);
    let e2 = sub(p[2], p[0]);
    let e3 = sub(p[3], p[0]);
    let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
    det.abs() / 6.0
}

/// The geometry factor `Γ = ∮|H_t|² dA / ∫|H|² dV` measured from the
/// element-constant discrete curl of a (full-edge) eigenvector — the ω²
/// in `H = ∇×E/(−iω)` cancels between numerator and denominator.
fn geometry_factor(fx: &BoxFixture, xf: &[f64]) -> f64 {
    let tet_edge_rows = fx.mesh.tet_edges();
    let mut num = 0.0_f64;
    for ((tri, &ti), &axis) in fx
        .wall_tris
        .iter()
        .zip(fx.wall_tet.iter())
        .zip(fx.wall_axis.iter())
    {
        let curl = tet_curl(&fx.mesh, &tet_edge_rows, ti, xf);
        let c2 = curl[0] * curl[0] + curl[1] * curl[1] + curl[2] * curl[2];
        let ht2 = c2 - curl[axis] * curl[axis];
        let p: [[f64; 3]; 3] = std::array::from_fn(|v| fx.mesh.nodes[tri[v] as usize]);
        num += triangle_area(&p) * ht2;
    }
    let mut den = 0.0_f64;
    for ti in 0..fx.mesh.n_tets() {
        let curl = tet_curl(&fx.mesh, &tet_edge_rows, ti, xf);
        let c2 = curl[0] * curl[0] + curl[1] * curl[1] + curl[2] * curl[2];
        let tet = fx.mesh.tets[ti];
        let p: [[f64; 3]; 4] = std::array::from_fn(|v| fx.mesh.nodes[tet[v] as usize]);
        den += tet_volume(&p) * c2;
    }
    assert!(den > 0.0);
    num / den
}

/// Normalized Euclidean overlap of two full-edge vectors (mode
/// identification: the physical London mode overlaps the PEC mode ≈ 1;
/// spurious penalty/gradient modes ≈ 0).
fn overlap(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    dot.abs() / (na * nb)
}

/// FEM kinetic-inductance shift of the London-wall box vs the first-order
/// perturbation oracle, across a decreasing-λ_L sweep (honest band).
#[test]
fn london_kinetic_inductance_shift_matches_perturbation_oracle() {
    let n = 5;
    let dims = [1.0, 0.85, 0.7];
    let fx = box_fixture(n, dims);
    let (k_vals, m_vals) = assemble_vacuum_pencil(&fx);
    let no_faces: Vec<[u32; 3]> = Vec::new();

    // Lowest resonance of the a × b × c PEC box uses the two largest
    // dimensions: λ₁ = π²(1/a² + 1/b²).
    let pi = std::f64::consts::PI;
    let lambda_analytic = pi * pi * (1.0 / (dims[0] * dims[0]) + 1.0 / (dims[1] * dims[1]));
    let sigma = 0.95 * lambda_analytic;
    let n_modes_pec = 4;
    let n_modes_london = 10; // cushion: the penalty spreads spurious gradient modes

    // --- PEC baseline: wall edges eliminated. ---
    let pencil_pec = TransmonPencil {
        scatter: &fx.scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt: bare_shunt(&no_faces),
        interior_mask: &fx.interior_mask_pec,
    };
    let modes_pec = solve_transmon_eigenmodes_full(
        &pencil_pec,
        sigma,
        n_modes_pec,
        M_PER_UNIT,
        InnerSolver::Direct,
    )
    .expect("PEC eigensolve");
    let pec = modes_pec
        .iter()
        .filter(|m| m.report.lambda > 1.0)
        .min_by(|a, b| a.report.lambda.partial_cmp(&b.report.lambda).unwrap())
        .expect("no physical PEC mode found")
        .clone();
    let lambda_pec = pec.report.lambda;
    let rel_analytic = (lambda_pec - lambda_analytic).abs() / lambda_analytic;
    eprintln!(
        "PEC λ₁ = {lambda_pec:.6} (analytic {lambda_analytic:.6}, rel diff {:.2}%)",
        100.0 * rel_analytic
    );
    assert!(
        rel_analytic < 0.10,
        "PEC baseline strays from the analytic box mode: {rel_analytic:.3}"
    );

    // --- Perturbation prediction from the PEC mode's discrete curl. ---
    let x_pec_full = scatter_full(&fx.interior_mask_pec, &pec.vector);
    let gamma = geometry_factor(&fx, &x_pec_full);
    eprintln!("geometry factor Γ = ∮|H_t|²dA / ∫|H|²dV = {gamma:.4}");
    assert!(gamma > 0.0);

    // --- Decreasing-λ_L sweep. Stop at 5e-3, well before λ_L⁻¹
    //     conditioning noise. ---
    let lambda_ls = [0.04, 0.02, 0.01, 0.005];
    let mut measured = Vec::new();
    let mut ratios = Vec::new();
    for &lambda_l in lambda_ls.iter() {
        let pencil = TransmonPencil {
            scatter: &fx.scatter,
            k_vals: &k_vals,
            m_vals: &m_vals,
            edges: &fx.edges,
            mesh: &fx.mesh,
            shunt: bare_shunt(&no_faces),
            interior_mask: &fx.keep_all_mask,
        };
        let walls = [LondonSurface {
            triangles: &fx.wall_tris,
            lambda_l,
        }];
        let modes = solve_transmon_eigenmodes_full_with_london(
            &pencil,
            &walls,
            sigma,
            n_modes_london,
            M_PER_UNIT,
            InnerSolver::Direct,
        )
        .expect("London eigensolve");

        // Identify the physical mode by overlap with the PEC mode.
        let best = modes
            .iter()
            .map(|m| {
                let xf = scatter_full(&fx.keep_all_mask, &m.vector);
                (overlap(&xf, &x_pec_full), m.report.lambda)
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .expect("no London modes");
        let (ov, lambda_london) = best;
        assert!(
            ov > 0.9,
            "physical-mode identification failed at λ_L = {lambda_l}: best overlap {ov:.3}"
        );

        let d_rel = (lambda_london.sqrt() - lambda_pec.sqrt()) / lambda_pec.sqrt();
        let predicted = -0.5 * lambda_l * gamma;
        let ratio = d_rel / predicted;
        eprintln!(
            "λ_L = {lambda_l:.3}: λ = {lambda_london:.6} (overlap {ov:.4}), \
             Δω/ω measured {d_rel:.5e}, predicted {predicted:.5e}, ratio {ratio:.4}"
        );
        assert!(
            d_rel < 0.0,
            "London wall must LOWER the frequency at λ_L = {lambda_l}: Δω/ω = {d_rel:.3e}"
        );
        measured.push(d_rel);
        ratios.push(ratio);
    }

    // Monotone: larger λ_L → weaker penalty → lower frequency (more
    // negative shift). The sweep is ordered decreasing in λ_L.
    for w in measured.windows(2) {
        assert!(
            w[0] < w[1],
            "kinetic-inductance shift must be monotone in λ_L: {:?}",
            measured
        );
    }

    // Honest agreement band: measured/predicted within ±20% at every
    // sweep point (first-order perturbation + O(h) discrete-curl bias on
    // the n = 5 mesh), and the smallest-λ_L point at least as close to 1
    // as the largest (convergence toward the oracle as λ_L → 0, with
    // slack for the discretization floor).
    for (lambda_l, ratio) in lambda_ls.iter().zip(ratios.iter()) {
        assert!(
            (ratio - 1.0).abs() < 0.20,
            "measured/predicted ratio {ratio:.4} outside the honest band at λ_L = {lambda_l}"
        );
    }
    let err_first = (ratios[0] - 1.0).abs();
    let err_last = (ratios[ratios.len() - 1] - 1.0).abs();
    eprintln!(
        "|ratio − 1|: λ_L = {} → {err_first:.4}, λ_L = {} → {err_last:.4}",
        lambda_ls[0],
        lambda_ls[lambda_ls.len() - 1]
    );
    assert!(
        err_last <= err_first + 0.02,
        "smallest-λ_L point must agree at least as well as the largest: \
         |ratio−1| {err_last:.4} vs {err_first:.4}"
    );
}
