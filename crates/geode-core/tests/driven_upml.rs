//! Driven-solve regression on an absorbing-boundary (UPML) case
//! (issue #194, acceptance criterion 4).
//!
//! Uses the bundled layered sphere fixture (dielectric sphere →
//! vacuum gap → UPML shell → outer PEC wall) and drives it with a
//! ẑ-polarized volumetric current confined to the dielectric core.
//! The UPML enters as the diagonal-anisotropic complex permittivity
//! from [`geode_core::assembly::nedelec::build_anisotropic_pml_tensor_diag`] — the same
//! material path the eigenpencil tests use — composed with the PEC
//! elimination on the outer wall, exercising the full
//! "PEC + UPML compose with the driven system" contract.
//!
//! # Assertions
//!
//! 1. The solve succeeds, every DOF is finite, and the direct-solve
//!    residual sits at the round-off floor.
//! 2. The radiated field **decays through the absorbing shell**: the
//!    mean per-length edge-field magnitude in the outer half of the
//!    PML is far below the mean over the source region. This is the
//!    driven-problem analog of the eigenmode tests' "the PML absorbs
//!    radiation" check.
//! 3. σ₀ = 0 regression: with absorption switched off, the
//!    DiagTensor UPML material must reproduce the scalar-ε material
//!    path (real dielectric everywhere) through the independent
//!    anisotropic assembly kernel.

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    build_anisotropic_pml_tensor_diag, build_complex_epsilon_r_pml, sphere_pec_interior_edges,
    tet_centroid_radii, tet_centroids,
};
use geode_core::testing::TestBackend;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, driven_solve};
use geode_core::mesh::{R_BUFFER, R_PML_INNER, R_SPHERE, read_sphere_fixture};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// ẑ-polarized current confined to the dielectric core (r < 0.5).
fn core_dipole_source(mesh: &geode_core::mesh::TetMesh) -> CurrentSource {
    CurrentSource::from_centroids(mesh, |c| {
        let r = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let jz = if r < 0.5 * R_SPHERE { 1.0 } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    })
}

/// Mean per-length |E·dl| over edges whose midpoint radius lies in
/// `[r_lo, r_hi)`. Normalizing the edge DOF (a line integral) by edge
/// length gives a field-strength proxy that is comparable across the
/// fixture's differently-sized mesh regions.
fn mean_field_in_shell(
    mesh: &geode_core::mesh::TetMesh,
    edges: &[[u32; 2]],
    e_edges: &[c64],
    r_lo: f64,
    r_hi: f64,
) -> f64 {
    let mut acc = 0.0_f64;
    let mut count = 0_usize;
    for (e, &dof) in edges.iter().zip(e_edges.iter()) {
        let p = mesh.nodes[e[0] as usize];
        let q = mesh.nodes[e[1] as usize];
        let mid = [
            0.5 * (p[0] + q[0]),
            0.5 * (p[1] + q[1]),
            0.5 * (p[2] + q[2]),
        ];
        let r = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2]).sqrt();
        if r < r_lo || r >= r_hi {
            continue;
        }
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt();
        acc += dof.re.hypot(dof.im) / len;
        count += 1;
    }
    assert!(
        count > 0,
        "no edges with midpoint radius in [{r_lo}, {r_hi})"
    );
    acc / count as f64
}

#[test]
fn driven_solve_with_pec_and_upml_absorbs_radiation() {
    let f = read_sphere_fixture().expect("fixture load");
    let edges = f.mesh.edges();

    let n_inside = 1.5_f64;
    let sigma_0 = 5.0_f64;
    // Drive near (but not on) the dielectric ground resonance
    // k ≈ π/(n·R) ≈ 2.1; the complex UPML keeps A(ω) well-conditioned
    // either way.
    let omega = 1.8_f64;

    let centroids = tet_centroids(&f.mesh);
    let eps_diag = build_anisotropic_pml_tensor_diag(
        &f.tet_physical_tags,
        &centroids,
        n_inside,
        sigma_0,
        omega,
    );

    let (_, interior) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let source = core_dipole_source(&f.mesh);

    let sol = driven_solve::<B>(
        &f.mesh,
        DrivenMaterials::DiagTensor(&eps_diag),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        omega,
        &source,
        &device(),
    )
    .expect("driven UPML solve");

    // 1. Numerical health.
    assert!(
        sol.e_edges
            .iter()
            .all(|e| e.re.is_finite() && e.im.is_finite()),
        "non-finite field values"
    );
    assert!(
        sol.residual_rel < 1e-8,
        "direct-solve residual too large: {}",
        sol.residual_rel
    );

    // 2. The field must decay through the absorbing shell: compare the
    //    source region (inside the dielectric) against the outer half
    //    of the UPML.
    let near = mean_field_in_shell(&f.mesh, &edges, &sol.e_edges, 0.0, R_SPHERE);
    let pml_outer = mean_field_in_shell(
        &f.mesh,
        &edges,
        &sol.e_edges,
        0.5 * (R_PML_INNER + R_BUFFER),
        R_BUFFER,
    );
    eprintln!(
        "mean |E| proxy: source region = {near:.4e}, outer PML half = {pml_outer:.4e}, \
         ratio = {:.4e}",
        pml_outer / near
    );
    assert!(near > 0.0, "source region field must be nonzero");
    assert!(
        pml_outer < 0.3 * near,
        "field does not decay through the UPML: outer-half mean {pml_outer:.4e} \
         vs source-region mean {near:.4e}"
    );
}

#[test]
fn upml_sigma_zero_matches_scalar_material_path() {
    let f = read_sphere_fixture().expect("fixture load");

    let n_inside = 1.5_f64;
    let omega = 1.8_f64;

    let centroids = tet_centroids(&f.mesh);
    let radii = tet_centroid_radii(&f.mesh);
    // σ₀ = 0: both builders reduce to the real dielectric profile
    // (ε = n² inside the sphere, 1 elsewhere).
    let eps_diag =
        build_anisotropic_pml_tensor_diag(&f.tet_physical_tags, &centroids, n_inside, 0.0, omega);
    let eps_scalar = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, n_inside, 0.0);

    let (_, interior) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = core_dipole_source(&f.mesh);

    let sol_diag = driven_solve::<B>(
        &f.mesh,
        DrivenMaterials::DiagTensor(&eps_diag),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("diag-tensor σ₀=0 solve");
    let sol_scalar = driven_solve::<B>(
        &f.mesh,
        DrivenMaterials::Scalar(&eps_scalar),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("scalar σ₀=0 solve");

    let norm: f64 = sol_scalar
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);
    let mut max_rel = 0.0_f64;
    for (a, b) in sol_scalar.e_edges.iter().zip(sol_diag.e_edges.iter()) {
        let d = *a - *b;
        max_rel = max_rel.max(d.re.hypot(d.im) / norm);
    }
    eprintln!("σ₀ = 0 scalar vs diag-tensor: max relative diff = {max_rel:.3e}");
    assert!(
        max_rel < 1e-4,
        "σ₀ = 0 UPML must reduce to the scalar dielectric path; \
         max relative diff {max_rel:.3e}"
    );
}
