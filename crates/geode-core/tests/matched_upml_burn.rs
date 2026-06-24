//! Validation of the Burn-path matched (full Sacks) UPML assembly
//! (issue #199): full-3×3 complex tensor weights on both the curl-curl
//! stiffness `K(ν = Λ⁻¹)` and the mass `M(ε = ε_r·Λ)`.
//!
//! Pins down:
//!
//! 1. **Reduction to the scalar path** — identity weights make the
//!    full-tensor assembler agree with the established complex-ε
//!    scalar assembler entrywise (independent kernels, same integral).
//! 2. **Reduction to the diag kernel** — a diagonal ε tensor agrees
//!    with [`geode_core::assemble_global_nedelec_with_anisotropic_epsilon`].
//! 3. **Λ-weighted complex symmetry** (acceptance criterion (c)) —
//!    `K(Λ⁻¹)ᵀ = K(Λ⁻¹)`, `M(ε_rΛ)ᵀ = M(ε_rΛ)`, and the full pencil
//!    `A(ω) = K(Λ⁻¹) + iωC(σ) − ω²M(ε_rΛ)` satisfies `A(ω)ᵀ = A(ω)`
//!    with σ₀ > 0 and σ > 0 (complex-symmetric, NOT Hermitian — same
//!    invariant as `tests/sigma_conductivity.rs`).
//! 4. **Autodiff** (acceptance criterion (e)) — gradients w.r.t. node
//!    coordinates flow through all four scattered outputs.
//! 5. **Input validation** — a wrong-length ν tensor errors, not
//!    panics.
//!
//! The Burn-vs-host assembly-equivalence tests (σ₀ ∈ {0, 25} against
//! [`geode_core::solve_scattered_field_matched_upml`]) live next to
//! the benchmark in `tests/mie_driven_scattering.rs`.

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Int, Tensor, TensorData};
use faer::c64;

use geode_core::{
    CurrentSource, DefaultBackend, DrivenBcs, DrivenError, DrivenMaterials, PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR, PHYS_VACUUM_GAP, R_PML_INNER, R_SPHERE, TetMesh,
    assemble_global_nedelec_with_anisotropic_epsilon, assemble_global_nedelec_with_complex_epsilon,
    assemble_global_nedelec_with_full_tensors, assemble_nedelec_sigma_damping,
    build_matched_upml_materials, cube_pec_interior_edges, cube_tet_mesh, driven_solve,
    tet_centroids, upload_mesh,
};

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

fn identity_tensor() -> [[c64; 3]; 3] {
    let mut w = [[c64::new(0.0, 0.0); 3]; 3];
    for (k, row) in w.iter_mut().enumerate() {
        row[k] = c64::new(1.0, 0.0);
    }
    w
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

fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .fold(0.0_f64, |acc, (&x, &y)| acc.max((x - y).abs()))
}

fn max_abs(a: &[f64]) -> f64 {
    a.iter().fold(0.0_f64, |acc, &x| acc.max(x.abs()))
}

/// Synthetic physical tags for a cube mesh spanning the matched-UPML
/// radial shell: interior / vacuum gap / PML shell by centroid radius.
fn radial_tags(mesh: &TetMesh) -> Vec<i32> {
    tet_centroids(mesh)
        .iter()
        .map(|c| {
            let r = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            if r < R_SPHERE {
                PHYS_SPHERE_INTERIOR
            } else if r < R_PML_INNER {
                PHYS_VACUUM_GAP
            } else {
                PHYS_PML_SHELL
            }
        })
        .collect()
}

/// Identity weights collapse the full-tensor assembler onto the
/// established scalar complex-ε path: `K(I)` must equal the real
/// curl-curl `K` (with zero imaginary part) and `M(ε·I)` the scalar
/// complex mass, entrywise. The two assemblers share no kernel code
/// for the weighted contraction, so this is a real cross-check.
#[test]
fn full_tensor_assembly_reduces_to_scalar_path_for_identity_weights() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &dev);

    let eps_scalar: Vec<c64> = (0..mesh.n_tets())
        .map(|e| c64::new(1.5 + 0.25 * (e % 4) as f64, -0.125 * (e % 3) as f64))
        .collect();
    let eps_tensor: Vec<[[c64; 3]; 3]> = eps_scalar
        .iter()
        .map(|&e| {
            let mut w = [[c64::new(0.0, 0.0); 3]; 3];
            for (k, row) in w.iter_mut().enumerate() {
                row[k] = e;
            }
            w
        })
        .collect();
    let nu_tensor: Vec<[[c64; 3]; 3]> = vec![identity_tensor(); mesh.n_tets()];

    let sys_full = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );
    let sys_scalar = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_scalar,
    );

    let k_re = readback_f64(sys_full.k_re);
    let k_im = readback_f64(sys_full.k_im);
    let m_re = readback_f64(sys_full.m_re);
    let m_im = readback_f64(sys_full.m_im);
    let k_ref = readback_f64(sys_scalar.k);
    let m_re_ref = readback_f64(sys_scalar.m_re);
    let m_im_ref = readback_f64(sys_scalar.m_im);

    let scale = max_abs(&k_ref).max(max_abs(&m_re_ref));
    assert!(scale > 0.0);
    let tol = 1e-5 * scale;

    assert!(
        max_abs_diff(&k_re, &k_ref) < tol,
        "K(I) disagrees with the scalar curl-curl K: {:.3e}",
        max_abs_diff(&k_re, &k_ref)
    );
    assert!(
        max_abs(&k_im) < tol,
        "K(I) must have zero imaginary part, got max {:.3e}",
        max_abs(&k_im)
    );
    assert!(
        max_abs_diff(&m_re, &m_re_ref) < tol,
        "Re M(ε·I) disagrees with the scalar complex mass: {:.3e}",
        max_abs_diff(&m_re, &m_re_ref)
    );
    assert!(
        max_abs_diff(&m_im, &m_im_ref) < tol,
        "Im M(ε·I) disagrees with the scalar complex mass: {:.3e}",
        max_abs_diff(&m_im, &m_im_ref)
    );
}

/// A purely diagonal ε tensor collapses the full-3×3 mass kernel onto
/// the established diagonal-anisotropic kernel.
#[test]
fn full_tensor_mass_reduces_to_diag_kernel_for_diagonal_weight() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &dev);

    let eps_diag: Vec<[c64; 3]> = (0..mesh.n_tets())
        .map(|e| {
            [
                c64::new(1.5, -0.5),
                c64::new(2.25 + 0.25 * (e % 2) as f64, 0.25),
                c64::new(0.75, -0.125),
            ]
        })
        .collect();
    let eps_tensor: Vec<[[c64; 3]; 3]> = eps_diag
        .iter()
        .map(|d| {
            let mut w = [[c64::new(0.0, 0.0); 3]; 3];
            for (k, row) in w.iter_mut().enumerate() {
                row[k] = d[k];
            }
            w
        })
        .collect();
    let nu_tensor: Vec<[[c64; 3]; 3]> = vec![identity_tensor(); mesh.n_tets()];

    let sys_full = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );
    let sys_diag = assemble_global_nedelec_with_anisotropic_epsilon::<B>(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_diag,
    );

    let m_re = readback_f64(sys_full.m_re);
    let m_im = readback_f64(sys_full.m_im);
    let m_re_ref = readback_f64(sys_diag.m_re);
    let m_im_ref = readback_f64(sys_diag.m_im);

    let scale = max_abs(&m_re_ref);
    assert!(scale > 0.0);
    let tol = 1e-5 * scale;
    assert!(
        max_abs_diff(&m_re, &m_re_ref) < tol,
        "Re M(diag ε) full-kernel vs diag-kernel mismatch: {:.3e}",
        max_abs_diff(&m_re, &m_re_ref)
    );
    assert!(
        max_abs_diff(&m_im, &m_im_ref) < tol,
        "Im M(diag ε) full-kernel vs diag-kernel mismatch: {:.3e}",
        max_abs_diff(&m_im, &m_im_ref)
    );
}

/// Acceptance criterion (c): the Λ-weighted pencil stays
/// complex-symmetric. `K(Λ⁻¹)ᵀ = K(Λ⁻¹)`, `M(ε_rΛ)ᵀ = M(ε_rΛ)`, and
/// `A(ω) = K(Λ⁻¹) + iωC(σ) − ω²M(ε_rΛ)` satisfies `A(ω)ᵀ = A(ω)` with
/// σ₀ > 0 (full Sacks stretch with off-diagonal Cartesian entries) and
/// a nonzero conductivity composed on top (#196).
#[test]
fn lambda_weighted_pencil_stays_complex_symmetric() {
    // Cube spanning [0, 2]³: centroid radii reach ≈ 3.2, so a healthy
    // fraction of tets sits beyond R_PML_INNER = 1.5 and picks up the
    // full (off-diagonal) Λ stretch.
    let mesh = cube_tet_mesh(2, 2.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &dev);

    let omega = 1.7;
    let sigma_0 = 25.0;
    let tags = radial_tags(&mesh);
    assert!(
        tags.contains(&PHYS_PML_SHELL),
        "test mesh must reach into the PML shell"
    );
    let (eps_tensor, nu_tensor) =
        build_matched_upml_materials(&mesh, &tags, PHYS_SPHERE_INTERIOR, 1.5, sigma_0, omega);

    let sys = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );
    // Spatially varying conductivity on top (σ-composition, #196).
    let sigma: Vec<f64> = tet_centroids(&mesh)
        .iter()
        .map(|c| 0.5 + 2.0 * c[0] + c[1])
        .collect();
    let c =
        assemble_nedelec_sigma_damping::<B>(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &sigma);

    let k_re = readback_f64(sys.k_re);
    let k_im = readback_f64(sys.k_im);
    let m_re = readback_f64(sys.m_re);
    let m_im = readback_f64(sys.m_im);
    let c = readback_f64(c);

    let scale = [&k_re, &k_im, &m_re, &m_im, &c]
        .iter()
        .fold(0.0_f64, |a, v| a.max(max_abs(v)));
    assert!(scale > 0.0);
    let tol = 1e-5 * scale;

    // The stretch must actually be active: K picks up an imaginary part.
    assert!(
        max_abs(&k_im) > tol,
        "Λ⁻¹ stretch produced no imaginary stiffness — test setup is degenerate"
    );

    assert!(
        max_asymmetry(&k_re, n_edges) < tol,
        "Re K(Λ⁻¹) lost symmetry"
    );
    assert!(
        max_asymmetry(&k_im, n_edges) < tol,
        "Im K(Λ⁻¹) lost symmetry"
    );
    assert!(
        max_asymmetry(&m_re, n_edges) < tol,
        "Re M(ε_rΛ) lost symmetry"
    );
    assert!(
        max_asymmetry(&m_im, n_edges) < tol,
        "Im M(ε_rΛ) lost symmetry"
    );

    // Full pencil A(ω) = K(Λ⁻¹) + iωC − ω²M(ε_rΛ): Aᵀ = A.
    let omega2 = omega * omega;
    let a_re: Vec<f64> = (0..n_edges * n_edges)
        .map(|i| k_re[i] - omega2 * m_re[i])
        .collect();
    let a_im: Vec<f64> = (0..n_edges * n_edges)
        .map(|i| k_im[i] + omega * c[i] - omega2 * m_im[i])
        .collect();
    assert!(
        max_asymmetry(&a_re, n_edges) < tol,
        "Re(A(ω)) not symmetric with Λ-weighted K and σ > 0"
    );
    assert!(
        max_asymmetry(&a_im, n_edges) < tol,
        "Im(A(ω)) not symmetric with Λ-weighted K and σ > 0"
    );
}

/// Acceptance criterion (e): autodiff smoke through the full-tensor
/// assembly — gradients w.r.t. node coordinates must exist, be finite,
/// and be nonzero somewhere for all four scattered outputs.
#[test]
fn full_tensor_assembly_preserves_autodiff() {
    use burn::backend::Autodiff;
    type Ad = Autodiff<B>;

    let mesh = cube_tet_mesh(2, 2.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let tags = radial_tags(&mesh);
    let (eps_tensor, nu_tensor) =
        build_matched_upml_materials(&mesh, &tags, PHYS_SPHERE_INTERIOR, 1.5, 25.0, 1.7);

    let n = mesh.n_nodes();
    let n_elem = mesh.n_tets();
    let ad_dev = <Ad as BackendTypes>::Device::default();
    let node_flat: Vec<f32> = mesh
        .nodes
        .iter()
        .flat_map(|p| p.iter().map(|&x| x as f32))
        .collect();
    let tet_flat: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| t.iter().map(|&i| i as i32))
        .collect();
    let nodes =
        Tensor::<Ad, 2>::from_data(TensorData::new(node_flat, [n, 3]), &ad_dev).require_grad();
    let tets = Tensor::<Ad, 2, Int>::from_data(TensorData::new(tet_flat, [n_elem, 4]), &ad_dev);

    let sys = assemble_global_nedelec_with_full_tensors::<Ad>(
        nodes.clone(),
        tets,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );
    let loss = sys.k_re.powf_scalar(2.0).sum()
        + sys.k_im.powf_scalar(2.0).sum()
        + sys.m_re.powf_scalar(2.0).sum()
        + sys.m_im.powf_scalar(2.0).sum();
    let grads = loss.backward();
    let dnodes = nodes
        .grad(&grads)
        .expect("gradient w.r.t. nodes should exist");
    let dnodes_vec: Vec<f64> = dnodes.into_data().iter::<f64>().collect();
    assert!(
        dnodes_vec.iter().all(|g| g.is_finite()),
        "all gradients must be finite"
    );
    assert!(
        dnodes_vec.iter().any(|g| g.abs() > 1e-6),
        "gradient should be non-zero somewhere"
    );
}

/// The degree-2 quadrature RHS ([`geode_core::QuadCurrentSource`] /
/// `driven_solve_quad`) must reduce to the per-tet-constant RHS path
/// for a constant-per-tet `J` — the rule integrates the Whitney basis
/// times a constant exactly, and the kernel collapses algebraically
/// (`A + 3B = 1`).
#[test]
fn quad_source_reduces_to_constant_source_for_constant_j() {
    use geode_core::{QuadCurrentSource, driven_solve_quad};

    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let eps: Vec<c64> = vec![c64::new(1.5, -0.05); mesh.n_tets()];
    let omega = 2.0;

    // Per-tet-constant but tet-varying J.
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(c[1], 0.0),
            c64::new(0.0, -0.5),
            c64::new(1.0, c[0]),
        ]
    });
    let quad_source = QuadCurrentSource::from_fn(&mesh, |t, _x| source.j_tet[t]);

    let sol_const = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("constant-J solve");
    let sol_quad = driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &quad_source,
        &device(),
    )
    .expect("quad-J solve");

    let norm: f64 = sol_const
        .e_edges
        .iter()
        .map(|e| e.norm_sqr())
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);
    let mut max_rel = 0.0_f64;
    for (a, b) in sol_const.e_edges.iter().zip(sol_quad.e_edges.iter()) {
        max_rel = max_rel.max((*a - *b).norm() / norm);
    }
    eprintln!("quad vs constant RHS (constant J): max relative diff = {max_rel:.3e}");
    assert!(
        max_rel < 1e-4,
        "degree-2 quadrature RHS must reduce to the constant-J RHS for \
         per-tet-constant J; max relative diff {max_rel:.3e}"
    );
}

/// A wrong-length ν tensor must error, not panic.
#[test]
fn matched_upml_dim_mismatch_errors() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let eps_tensor: Vec<[[c64; 3]; 3]> = vec![identity_tensor(); mesh.n_tets()];
    let bad_nu: Vec<[[c64; 3]; 3]> = vec![identity_tensor(); 2];

    let err = driven_solve::<B>(
        &mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_tensor,
            nu_tensor: &bad_nu,
        },
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::MaterialDimMismatch { .. }));
}
