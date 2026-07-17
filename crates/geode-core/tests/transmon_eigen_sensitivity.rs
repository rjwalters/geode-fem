//! Hellmann–Feynman eigenvalue-sensitivity `∂λ/∂p` finite-difference
//! validation (Epic #569 eigen leg, issue #596 — **Phase A**).
//!
//! For a **simple** eigenvalue of the real symmetric-definite transmon pencil
//! `K x = λ M x`, [`geode_core::eigen::sensitivity`] computes `∂λ/∂ε` (material)
//! and `∂λ/∂θ` (geometry) analytically from the *converged eigenpair alone* —
//! no adjoint solve. These tests confirm the analytic gradients against a
//! central finite difference that **re-solves the same `InnerSolver::Direct`
//! eigensolver** for the same mode, exactly the pattern the material/shape
//! sensitivity epic (#570/#571/#576/#577) used.
//!
//! - **CI-fast:** a small dielectric PEC-cavity fixture (bare cavity, no port),
//!   both material and geometry, plus the simple-eigenvalue gap guard and the
//!   zero-volume-region edge case.
//! - **Release / `#[ignore]`:** the real 133k-tet DeviceLayout mesh, the
//!   honesty/scale cross-check.

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::lanczos::InnerSolver;
use geode_core::eigen::sensitivity::{EigenSensitivity, EigenSensitivityError};
use geode_core::eigen::transmon::{
    LumpedReactiveShunt, ReactiveElementNatural, TransmonMode, TransmonPencil,
    solve_transmon_eigenmodes_full,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TetMesh, TransmonFixture, cube_tet_mesh, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

const M_PER_UNIT: f64 = 1e-6;

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

/// Assemble the REAL Nédélec pencil value vectors `(k_vals, m_vals)` for a
/// per-tet **isotropic** real permittivity `eps_r` (μ_r = 1, lossless). The
/// mass is `M = Σ_t ε_r[t] ∫ N·N` — the linear-in-ε structure the material
/// sensitivity exploits.
fn assemble_iso_pencil(
    mesh: &TetMesh,
    tet_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    eps_r: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu_tensor = vec![identity; mesh.n_tets()];
    let epsilon_tensor: Vec<[[c64; 3]; 3]> = eps_r
        .iter()
        .map(|&e| {
            std::array::from_fn(|i| {
                std::array::from_fn(|j| c64::new(if i == j { e } else { 0.0 }, 0.0))
            })
        })
        .collect();

    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        tet_sign,
        scatter,
        &epsilon_tensor,
        &nu_tensor,
    );
    (vals_to_host(sys.k_re_vals), vals_to_host(sys.m_re_vals))
}

/// A closed dielectric PEC-cavity fixture: an `n×n×n` unit cube, PEC on the
/// entire outer boundary, uniform isotropic ε. NO junction port (a bare cavity)
/// so the geometry gradient is purely the volume curl-curl/mass terms and the
/// eigenvector is M-normalized w.r.t. the plain mass.
struct CavityFixture {
    mesh: TetMesh,
    tet_edge_idx: Vec<[u32; 6]>,
    tet_edge_sign: Vec<[i8; 6]>,
    edges: Vec<[u32; 2]>,
    interior_mask: Vec<bool>,
}

fn cavity_fixture(n: usize) -> CavityFixture {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&mesh);

    // PEC on every outer-boundary face (a fully closed cavity).
    let metal: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            let on = |c: usize, val: f64| {
                f.iter()
                    .all(|&v| (mesh.nodes[v as usize][c] - val).abs() < 1e-12)
            };
            on(0, 0.0) || on(0, 1.0) || on(1, 0.0) || on(1, 1.0) || on(2, 0.0) || on(2, 1.0)
        })
        .collect();
    let interior_mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);

    CavityFixture {
        mesh,
        tet_edge_idx,
        tet_edge_sign,
        edges,
        interior_mask,
    }
}

/// Bare-cavity shunt (no port): `L = ∞` (k_scale 0) and `C = 0` (m_scale 0),
/// so no surface terms are added — the pencil is the pure volume `(K, M(ε))`.
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

/// Solve the bare cavity with per-tet isotropic `eps_r`, returning the full
/// [`TransmonMode`]s (reports + reduced M-normalized eigenvectors) via the
/// `Direct` eigensolver.
fn solve_cavity(
    fx: &CavityFixture,
    eps_r: &[f64],
    sigma: f64,
    n_modes: usize,
) -> Vec<TransmonMode> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) = assemble_iso_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, eps_r);
    let no_faces: Vec<[u32; 3]> = Vec::new();
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt: bare_shunt(&no_faces),
        interior_mask: &fx.interior_mask,
    };
    solve_transmon_eigenmodes_full(&pencil, sigma, n_modes, M_PER_UNIT, InnerSolver::Direct)
        .expect("cavity eigensolve")
}

/// Pick the returned mode with the largest relative gap to its neighbors that
/// is well above the nullspace — the safest "simple" mode for the HF test.
/// Returns `(index_in_list, lambdas_vec, chosen_mode)`.
fn pick_simple_mode(modes: &[TransmonMode]) -> (usize, Vec<f64>, TransmonMode) {
    let lambdas: Vec<f64> = modes.iter().map(|m| m.report.lambda).collect();
    let mut best = (0usize, f64::NEG_INFINITY);
    for (i, &li) in lambdas.iter().enumerate() {
        if li <= 1e-3 {
            continue; // skip nullspace-adjacent modes
        }
        let denom = li.abs().max(f64::MIN_POSITIVE);
        let mut gap = f64::INFINITY;
        for (j, &lj) in lambdas.iter().enumerate() {
            if i != j {
                gap = gap.min((li - lj).abs() / denom);
            }
        }
        if gap > best.1 {
            best = (i, gap);
        }
    }
    let idx = best.0;
    (idx, lambdas, modes[idx].clone())
}

/// Central-difference the eigenvalue of the mode nearest `lambda_base` under a
/// mesh + material perturbation, re-solving the same `Direct` eigensolver.
#[allow(clippy::too_many_arguments)]
fn fd_lambda(
    fx_mesh: &TetMesh,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    edges: &[[u32; 2]],
    interior_mask: &[bool],
    eps_r: &[f64],
    sigma: f64,
    n_modes: usize,
    lambda_base: f64,
) -> f64 {
    let scatter = NedelecScatterMap::new(tet_edge_idx);
    let (k_vals, m_vals) = assemble_iso_pencil(fx_mesh, tet_edge_sign, &scatter, eps_r);
    let no_faces: Vec<[u32; 3]> = Vec::new();
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges,
        mesh: fx_mesh,
        shunt: bare_shunt(&no_faces),
        interior_mask,
    };
    let modes =
        solve_transmon_eigenmodes_full(&pencil, sigma, n_modes, M_PER_UNIT, InnerSolver::Direct)
            .expect("FD eigensolve");
    // Track the mode by nearest eigenvalue (small perturbation ⇒ continuous).
    modes
        .iter()
        .map(|m| m.report.lambda)
        .min_by(|a, b| {
            (a - lambda_base)
                .abs()
                .partial_cmp(&(b - lambda_base).abs())
                .unwrap()
        })
        .expect("no FD modes")
}

// -------------------------------------------------------------------------
// CI-fast: material sensitivity ∂λ/∂ε.
// -------------------------------------------------------------------------

/// `∂λ/∂ε` (material) FD-validated on the synthetic cavity: the analytic
/// Hellmann–Feynman gradient matches a central finite difference of the
/// re-solved eigenvalue, and the uniform-ε closed form `∂λ/∂ε = −λ/ε`.
#[test]
fn material_sensitivity_matches_fd_and_closed_form() {
    let fx = cavity_fixture(3);
    let eps0 = 4.0_f64;
    let n_tets = fx.mesh.n_tets();
    let eps_r = vec![eps0; n_tets];
    let sigma = 5.0;
    let n_modes = 6;

    let modes = solve_cavity(&fx, &eps_r, sigma, n_modes);
    let (idx, lambdas, mode) = pick_simple_mode(&modes);
    let lambda = mode.report.lambda;
    assert!(lambda > 1e-3, "picked λ = {lambda} too close to nullspace");

    let sens = EigenSensitivity {
        mesh: &fx.mesh,
        edges: &fx.edges,
        interior_mask: &fx.interior_mask,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: idx,
        eigenvector: &mode.vector,
        min_rel_gap: 1e-2,
    };

    // Uniform region (all tets → region 0): analytic HF gradient.
    let region_of = vec![0usize; n_tets];
    let grad = sens.deigenvalue_deps(&region_of, 1).expect("material grad");
    let analytic = grad[0];

    // Closed form for a uniform ε: ∂λ/∂ε = −λ/ε.
    let closed_form = -lambda / eps0;
    let rel_cf = (analytic - closed_form).abs() / closed_form.abs();
    assert!(
        rel_cf < 1e-6,
        "uniform ∂λ/∂ε analytic {analytic:.6e} vs closed form −λ/ε {closed_form:.6e} \
         (rel {rel_cf:.2e})"
    );

    // Central FD of the re-solved eigenvalue under a uniform ε perturbation.
    let h = 1e-4 * eps0;
    let eps_p: Vec<f64> = eps_r.iter().map(|&e| e + h).collect();
    let eps_m: Vec<f64> = eps_r.iter().map(|&e| e - h).collect();
    let lam_p = fd_lambda(
        &fx.mesh,
        &fx.tet_edge_idx,
        &fx.tet_edge_sign,
        &fx.edges,
        &fx.interior_mask,
        &eps_p,
        sigma,
        n_modes,
        lambda,
    );
    let lam_m = fd_lambda(
        &fx.mesh,
        &fx.tet_edge_idx,
        &fx.tet_edge_sign,
        &fx.edges,
        &fx.interior_mask,
        &eps_m,
        sigma,
        n_modes,
        lambda,
    );
    let fd = (lam_p - lam_m) / (2.0 * h);
    let rel_fd = (analytic - fd).abs() / fd.abs().max(1e-30);
    eprintln!(
        "material: analytic {analytic:.6e}, FD {fd:.6e}, closed-form {closed_form:.6e} \
         (rel_fd {rel_fd:.2e})"
    );
    assert!(
        rel_fd < 1e-4,
        "∂λ/∂ε analytic {analytic:.6e} vs FD {fd:.6e} (rel {rel_fd:.2e})"
    );
}

/// Per-region `∂λ/∂ε_k`: split the cube into two regions by centroid `x`, FD
/// each region independently, and verify the two region gradients sum to the
/// uniform closed form `−λ/ε`. Also the zero-volume-region edge case → 0.
#[test]
fn per_region_material_sensitivity_matches_fd() {
    let fx = cavity_fixture(3);
    let eps0 = 4.0_f64;
    let n_tets = fx.mesh.n_tets();
    let eps_r = vec![eps0; n_tets];
    let sigma = 5.0;
    let n_modes = 6;

    // Region 0 = tets with centroid x < 0.5, region 1 otherwise.
    let region_of: Vec<usize> = (0..n_tets)
        .map(|t| {
            let tet = &fx.mesh.tets[t];
            let cx = (0..4)
                .map(|k| fx.mesh.nodes[tet[k] as usize][0])
                .sum::<f64>()
                / 4.0;
            if cx < 0.5 { 0 } else { 1 }
        })
        .collect();

    let modes = solve_cavity(&fx, &eps_r, sigma, n_modes);
    let (idx, lambdas, mode) = pick_simple_mode(&modes);
    let lambda = mode.report.lambda;

    // Use 3 regions so region 2 has NO tets (zero-volume edge case).
    let n_regions = 3;
    let sens = EigenSensitivity {
        mesh: &fx.mesh,
        edges: &fx.edges,
        interior_mask: &fx.interior_mask,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: idx,
        eigenvector: &mode.vector,
        min_rel_gap: 1e-2,
    };
    let grad = sens
        .deigenvalue_deps(&region_of, n_regions)
        .expect("region grad");

    // Zero-volume region → exactly 0 (not NaN).
    assert_eq!(grad[2], 0.0, "empty region 2 must have zero sensitivity");
    assert!(grad[2].is_finite());

    // Sum of region gradients == uniform ∂λ/∂ε = −λ/ε.
    let sum = grad[0] + grad[1];
    let closed_form = -lambda / eps0;
    assert!(
        (sum - closed_form).abs() / closed_form.abs() < 1e-6,
        "Σ region grads {sum:.6e} != uniform −λ/ε {closed_form:.6e}"
    );

    // FD each region separately (perturb only that region's ε).
    #[allow(clippy::needless_range_loop)]
    for k in 0..2 {
        let h = 1e-4 * eps0;
        let bump = |sign: f64| -> Vec<f64> {
            (0..n_tets)
                .map(|t| eps_r[t] + if region_of[t] == k { sign * h } else { 0.0 })
                .collect()
        };
        let lam_p = fd_lambda(
            &fx.mesh,
            &fx.tet_edge_idx,
            &fx.tet_edge_sign,
            &fx.edges,
            &fx.interior_mask,
            &bump(1.0),
            sigma,
            n_modes,
            lambda,
        );
        let lam_m = fd_lambda(
            &fx.mesh,
            &fx.tet_edge_idx,
            &fx.tet_edge_sign,
            &fx.edges,
            &fx.interior_mask,
            &bump(-1.0),
            sigma,
            n_modes,
            lambda,
        );
        let fd = (lam_p - lam_m) / (2.0 * h);
        let rel = (grad[k] - fd).abs() / fd.abs().max(1e-30);
        eprintln!(
            "region {k}: analytic {:.6e}, FD {fd:.6e} (rel {rel:.2e})",
            grad[k]
        );
        assert!(
            rel < 5e-4,
            "region {k} ∂λ/∂ε_k analytic {:.6e} vs FD {fd:.6e} (rel {rel:.2e})",
            grad[k]
        );
    }
}

// -------------------------------------------------------------------------
// CI-fast: geometry sensitivity ∂λ/∂θ.
// -------------------------------------------------------------------------

/// Move the mesh node whose position is nearest a given target.
fn nearest_interior_node(mesh: &TetMesh, target: [f64; 3]) -> usize {
    // Prefer a strictly-interior node (all coords in (eps, 1-eps)) so its
    // adjacent tets never touch a boundary face.
    let eps = 1e-9;
    let mut best = (0usize, f64::INFINITY);
    for (i, p) in mesh.nodes.iter().enumerate() {
        let interior = p.iter().all(|&c| c > eps && c < 1.0 - eps);
        if !interior {
            continue;
        }
        let d = (0..3).map(|k| (p[k] - target[k]).powi(2)).sum::<f64>();
        if d < best.1 {
            best = (i, d);
        }
    }
    assert!(best.1.is_finite(), "no strictly-interior node found");
    best.0
}

/// `∂λ/∂θ` (geometry) FD-validated on the synthetic cavity: moving a single
/// interior node along `x`, the analytic Hellmann–Feynman node-motion gradient
/// matches a central finite difference of the re-solved eigenvalue.
#[test]
fn geometry_sensitivity_matches_fd() {
    let fx = cavity_fixture(3);
    let eps0 = 4.0_f64;
    let n_tets = fx.mesh.n_tets();
    let eps_r = vec![eps0; n_tets];
    let sigma = 5.0;
    let n_modes = 6;

    let modes = solve_cavity(&fx, &eps_r, sigma, n_modes);
    let (idx, lambdas, mode) = pick_simple_mode(&modes);
    let lambda = mode.report.lambda;
    assert!(lambda > 1e-3);

    // Node-motion velocity: move one interior node along +x by θ.
    let node = nearest_interior_node(&fx.mesh, [0.5, 0.5, 0.5]);
    let mut vel = vec![[0.0_f64; 3]; fx.mesh.n_nodes()];
    vel[node] = [1.0, 0.0, 0.0];

    let sens = EigenSensitivity {
        mesh: &fx.mesh,
        edges: &fx.edges,
        interior_mask: &fx.interior_mask,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: idx,
        eigenvector: &mode.vector,
        min_rel_gap: 1e-2,
    };
    let analytic = sens.deigenvalue_dtheta(&vel).expect("geometry grad");

    // Central FD: move the node ±h along the velocity, re-solve.
    let h = 1e-5;
    let moved = |sign: f64| -> TetMesh {
        let mut m = fx.mesh.clone();
        for (n, v) in vel.iter().enumerate() {
            m.nodes[n][0] += sign * h * v[0];
            m.nodes[n][1] += sign * h * v[1];
            m.nodes[n][2] += sign * h * v[2];
        }
        m
    };
    let mesh_p = moved(1.0);
    let mesh_m = moved(-1.0);
    let lam_p = fd_lambda(
        &mesh_p,
        &fx.tet_edge_idx,
        &fx.tet_edge_sign,
        &fx.edges,
        &fx.interior_mask,
        &eps_r,
        sigma,
        n_modes,
        lambda,
    );
    let lam_m = fd_lambda(
        &mesh_m,
        &fx.tet_edge_idx,
        &fx.tet_edge_sign,
        &fx.edges,
        &fx.interior_mask,
        &eps_r,
        sigma,
        n_modes,
        lambda,
    );
    let fd = (lam_p - lam_m) / (2.0 * h);
    let rel = (analytic - fd).abs() / fd.abs().max(1e-30);
    eprintln!("geometry: analytic {analytic:.6e}, FD {fd:.6e} (rel {rel:.2e})");
    assert!(
        rel < 5e-3,
        "∂λ/∂θ analytic {analytic:.6e} vs FD {fd:.6e} (rel {rel:.2e})"
    );
}

// -------------------------------------------------------------------------
// CI-fast: simple-eigenvalue guard.
// -------------------------------------------------------------------------

/// The simple-eigenvalue precondition trips rather than silently producing a
/// wrong gradient: with an unreachable `min_rel_gap`, every entry point returns
/// `DegenerateEigenvalue`; with a sane gap, the same mode succeeds.
#[test]
fn degenerate_gap_guard_trips() {
    let fx = cavity_fixture(3);
    let eps0 = 4.0_f64;
    let n_tets = fx.mesh.n_tets();
    let eps_r = vec![eps0; n_tets];
    let sigma = 5.0;
    let n_modes = 6;

    let modes = solve_cavity(&fx, &eps_r, sigma, n_modes);
    let (idx, lambdas, mode) = pick_simple_mode(&modes);

    // Unreachable gap → guard trips on ALL entry points.
    let strict = EigenSensitivity {
        mesh: &fx.mesh,
        edges: &fx.edges,
        interior_mask: &fx.interior_mask,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: idx,
        eigenvector: &mode.vector,
        min_rel_gap: 1e9,
    };
    let region_of = vec![0usize; n_tets];
    assert!(matches!(
        strict.deigenvalue_deps(&region_of, 1),
        Err(EigenSensitivityError::DegenerateEigenvalue { .. })
    ));
    assert!(matches!(
        strict.deigenvalue_dx(),
        Err(EigenSensitivityError::DegenerateEigenvalue { .. })
    ));
    let vel = vec![[0.0_f64; 3]; fx.mesh.n_nodes()];
    assert!(matches!(
        strict.deigenvalue_dtheta(&vel),
        Err(EigenSensitivityError::DegenerateEigenvalue { .. })
    ));

    // Sane gap → the picked (simple) mode succeeds.
    let ok = EigenSensitivity {
        min_rel_gap: 1e-2,
        ..strict
    };
    assert!(ok.deigenvalue_deps(&region_of, 1).is_ok());
}

// -------------------------------------------------------------------------
// Release / #[ignore]: real 133k-tet fixture scale cross-check.
// -------------------------------------------------------------------------

/// Assemble the real-mesh bare-cavity pencil (uniform isotropic ε) and solve
/// `n_modes` near `sigma_f_hz` via `Direct`.
#[allow(clippy::type_complexity)]
fn solve_real_bare_cavity(
    f: &TransmonFixture,
    eps_r: &[f64],
    sigma: f64,
    n_modes: usize,
) -> (
    Vec<[u32; 2]>,
    Vec<bool>,
    Vec<[u32; 6]>,
    Vec<[i8; 6]>,
    Vec<TransmonMode>,
) {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_iso_pencil(&f.mesh, &tet_edge_sign, &scatter, eps_r);
    let no_faces: Vec<[u32; 3]> = Vec::new();
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt: bare_shunt(&no_faces),
        interior_mask: &interior_mask,
    };
    let modes =
        solve_transmon_eigenmodes_full(&pencil, sigma, n_modes, M_PER_UNIT, InnerSolver::Direct)
            .expect("real bare-cavity eigensolve");
    (edges, interior_mask, tet_edge_idx, tet_edge_sign, modes)
}

/// Release-tier honesty/scale cross-check: on the real 133k-tet DeviceLayout
/// mesh (bare dielectric cavity, uniform isotropic ε), the analytic
/// Hellmann–Feynman `∂λ/∂ε` and `∂λ/∂θ` match a central finite difference of
/// the re-solved `Direct` eigensolver on a picked simple mode.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigen_sensitivity \
///     -- --ignored real_eigen_sensitivity_release --nocapture
/// ```
#[test]
#[ignore = "two+ 133k-DOF sparse shift-invert eigensolves (FD) — release benchmark only"]
fn real_eigen_sensitivity_release() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    eprintln!(
        "fixture: {} nodes, {} tets",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );
    let eps0 = 10.0_f64; // isotropic dielectric (scale cross-check, not sapphire)
    let n_tets = f.mesh.n_tets();
    let eps_r = vec![eps0; n_tets];
    // Target a low physical band on the μm mesh; request several modes to find
    // a well-separated simple one above the gradient nullspace.
    let sigma = geode_core::eigen::transmon::lambda_shift_for_frequency_hz(5.0e9, M_PER_UNIT);
    let n_modes = 8;

    let (edges, interior_mask, tet_edge_idx, tet_edge_sign, modes) =
        solve_real_bare_cavity(&f, &eps_r, sigma, n_modes);
    let (idx, lambdas, mode) = pick_simple_mode(&modes);
    let lambda = mode.report.lambda;
    eprintln!(
        "picked mode {idx}: λ = {lambda:.6e}, f = {:.4} GHz",
        mode.report.frequency_ghz()
    );
    assert!(lambda > 0.0);

    let sens = EigenSensitivity {
        mesh: &f.mesh,
        edges: &edges,
        interior_mask: &interior_mask,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: idx,
        eigenvector: &mode.vector,
        min_rel_gap: 1e-3,
    };

    // --- Material: uniform ∂λ/∂ε vs FD and closed form −λ/ε. ---
    let region_of = vec![0usize; n_tets];
    let grad = sens.deigenvalue_deps(&region_of, 1).expect("material grad");
    let analytic_m = grad[0];
    let closed_form = -lambda / eps0;
    let h = 1e-4 * eps0;
    let lam_p = {
        let eps: Vec<f64> = eps_r.iter().map(|&e| e + h).collect();
        fd_lambda(
            &f.mesh,
            &tet_edge_idx,
            &tet_edge_sign,
            &edges,
            &interior_mask,
            &eps,
            sigma,
            n_modes,
            lambda,
        )
    };
    let lam_m = {
        let eps: Vec<f64> = eps_r.iter().map(|&e| e - h).collect();
        fd_lambda(
            &f.mesh,
            &tet_edge_idx,
            &tet_edge_sign,
            &edges,
            &interior_mask,
            &eps,
            sigma,
            n_modes,
            lambda,
        )
    };
    let fd_m = (lam_p - lam_m) / (2.0 * h);
    eprintln!("material: analytic {analytic_m:.6e}, FD {fd_m:.6e}, closed-form {closed_form:.6e}");
    assert!(
        (analytic_m - closed_form).abs() / closed_form.abs() < 1e-6,
        "uniform ∂λ/∂ε vs −λ/ε mismatch"
    );
    assert!(
        (analytic_m - fd_m).abs() / fd_m.abs() < 1e-3,
        "material ∂λ/∂ε analytic {analytic_m:.6e} vs FD {fd_m:.6e}"
    );

    // --- Geometry: move one interior node along +x, ∂λ/∂θ vs FD. ---
    let node = nearest_interior_node(&f.mesh, {
        // centroid of the mesh bounding box
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &f.mesh.nodes {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        [
            0.5 * (lo[0] + hi[0]),
            0.5 * (lo[1] + hi[1]),
            0.5 * (lo[2] + hi[2]),
        ]
    });
    let mut vel = vec![[0.0_f64; 3]; f.mesh.n_nodes()];
    vel[node] = [1.0, 0.0, 0.0];
    let analytic_g = sens.deigenvalue_dtheta(&vel).expect("geometry grad");
    let hg = 1e-3; // μm-unit mesh: a small absolute node motion
    let moved = |sign: f64| -> TetMesh {
        let mut m = f.mesh.clone();
        m.nodes[node][0] += sign * hg;
        m
    };
    let mesh_p = moved(1.0);
    let mesh_m = moved(-1.0);
    let lam_gp = fd_lambda(
        &mesh_p,
        &tet_edge_idx,
        &tet_edge_sign,
        &edges,
        &interior_mask,
        &eps_r,
        sigma,
        n_modes,
        lambda,
    );
    let lam_gm = fd_lambda(
        &mesh_m,
        &tet_edge_idx,
        &tet_edge_sign,
        &edges,
        &interior_mask,
        &eps_r,
        sigma,
        n_modes,
        lambda,
    );
    let fd_g = (lam_gp - lam_gm) / (2.0 * hg);
    let rel_g = (analytic_g - fd_g).abs() / fd_g.abs().max(1e-30);
    eprintln!("geometry: analytic {analytic_g:.6e}, FD {fd_g:.6e} (rel {rel_g:.2e})");
    assert!(
        rel_g < 1e-2,
        "geometry ∂λ/∂θ analytic {analytic_g:.6e} vs FD {fd_g:.6e} (rel {rel_g:.2e})"
    );
}
