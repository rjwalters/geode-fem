//! Criterion benchmark: full Mie-sphere FEM pipeline at the bundled
//! fixture's resolution (one bench point).
//!
//! Stages timed together as a single black-box:
//!
//! 1. Upload sphere mesh tensors to the active backend.
//! 2. `assemble_global_nedelec_with_complex_epsilon` with the same
//!    PML ε field that `examples/mie_sphere.rs` uses.
//! 3. Convert K and complex M back to faer host matrices.
//! 4. Apply the PEC interior mask (Dirichlet BC reduction).
//! 5. Hand the complex pencil to `FaerComplexEigensolver` and request
//!    `spurious_dim + 5` eigenvalues (matching the example, minus the
//!    extra padding it requests purely for spurious-mode budget).
//!
//! Sample size is set low because each call takes several seconds —
//! this single bench point alone dominates the cargo-bench wall-clock
//! budget.

use std::time::Duration;

use burn::tensor::backend::BackendTypes;
use criterion::{Criterion, criterion_group, criterion_main};

use faer::sparse::{SparseColMat, Triplet};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Refractive index inside the sphere. Matches `examples/mie_sphere.rs`.
const N_INSIDE: f64 = 1.5;
/// PML absorption strength.
const SIGMA_0: f64 = 5.0;
/// Physical mode count above the spurious gradient nullspace.
const N_MODES: usize = 8;

fn bench_mie_end_to_end_dense(c: &mut Criterion) {
    let mut group = c.benchmark_group("mie_end_to_end_dense");
    // Each iteration is multi-second; cap samples to keep total
    // wall-clock under the bench-suite budget.
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(40));

    // Load the fixture once. The bench body re-uploads tensors and
    // re-assembles each iteration, which is the realistic per-call
    // cost of the Mie pipeline.
    let fixture = read_sphere_fixture().expect("sphere fixture load");
    let radii = tet_centroid_radii(&fixture.mesh);
    let eps_complex =
        build_complex_epsilon_r_pml(&fixture.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);

    let tet_edges_idx = fixture.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let n_edges = fixture.mesh.edges().len();

    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    let spurious_dim = sphere_n_interior_nodes(&fixture.mesh, R_BUFFER);
    let n_request = spurious_dim + N_MODES + 5;

    let device = <B as BackendTypes>::Device::default();

    group.bench_function("sphere_fixture", |b| {
        b.iter(|| {
            // 1. Upload mesh tensors.
            let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);

            // 2. Complex-ε Nédélec assembly.
            let sys = assemble_global_nedelec_with_complex_epsilon(
                nodes_t,
                tets_t,
                &tet_idx,
                &tet_sign,
                n_edges,
                &eps_complex,
            );

            // 3. Pull dense matrices to host.
            let k_full = burn_matrix_to_faer(sys.k);
            let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

            // 4. PEC reduction.
            let dummy = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
            let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy.as_ref(), &interior_mask)
                .expect("BC reduction");
            let interior_idx: Vec<usize> = interior_mask
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b { Some(i) } else { None })
                .collect();
            let dim = interior_idx.len();
            let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
                m_complex_full[(interior_idx[i], interior_idx[j])]
            });
            let k_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
                faer::c64::new(k_int[(i, j)], 0.0)
            });

            // 5. Complex eigensolve for the lowest `n_request` modes.
            let lambdas = FaerComplexEigensolver
                .smallest_complex_pencil_eigenvalues(
                    k_int_complex.as_ref(),
                    m_int_complex.as_ref(),
                    n_request,
                )
                .expect("complex eigensolve");
            criterion::black_box(lambdas);
        });
    });
    group.finish();
}

/// Sparse complex shift-and-invert Lanczos variant of the Mie pipeline
/// (issue #53). Identical end-to-end stages 1–4 to
/// `bench_mie_end_to_end_dense`; step 5 swaps in
/// [`SparseComplexShiftInvertLanczos`] over the CSC projection of the
/// pencil. Expected speedup: ~100× at the bundled fixture size.
fn bench_mie_end_to_end_sparse(c: &mut Criterion) {
    let mut group = c.benchmark_group("mie_end_to_end_sparse");
    // Sparse path is fast; we can afford the normal sample count.
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(20));

    let fixture = read_sphere_fixture().expect("sphere fixture load");
    let radii = tet_centroid_radii(&fixture.mesh);
    let eps_complex =
        build_complex_epsilon_r_pml(&fixture.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);

    let tet_edges_idx = fixture.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let n_edges = fixture.mesh.edges().len();

    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    let spurious_dim = sphere_n_interior_nodes(&fixture.mesh, R_BUFFER);
    let n_request = spurious_dim + N_MODES + 5;

    let device = <B as BackendTypes>::Device::default();

    group.bench_function("sphere_fixture", |b| {
        b.iter(|| {
            let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);
            let sys = assemble_global_nedelec_with_complex_epsilon(
                nodes_t,
                tets_t,
                &tet_idx,
                &tet_sign,
                n_edges,
                &eps_complex,
            );

            let k_full = burn_matrix_to_faer(sys.k);
            let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

            let dummy = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
            let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy.as_ref(), &interior_mask)
                .expect("BC reduction");
            let interior_idx: Vec<usize> = interior_mask
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| if b { Some(i) } else { None })
                .collect();
            let dim = interior_idx.len();
            let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
                m_complex_full[(interior_idx[i], interior_idx[j])]
            });
            let k_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
                faer::c64::new(k_int[(i, j)], 0.0)
            });

            // Sparsify: walk dense entries, keep non-zeros.
            let mut k_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
            let mut m_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
            for j in 0..dim {
                for i in 0..dim {
                    let kv = k_int_complex[(i, j)];
                    if kv.re != 0.0 || kv.im != 0.0 {
                        k_trips.push(Triplet::new(i, j, kv));
                    }
                    let mv = m_int_complex[(i, j)];
                    if mv.re != 0.0 || mv.im != 0.0 {
                        m_trips.push(Triplet::new(i, j, mv));
                    }
                }
            }
            let k_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &k_trips)
                .expect("sparse K");
            let m_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &m_trips)
                .expect("sparse M");

            let lambdas = SparseComplexShiftInvertLanczos {
                sigma: 0.0,
                max_iters: 256,
                tol: 1e-9,
            }
            .smallest_complex_pencil_eigenvalues(k_sp.as_ref(), m_sp.as_ref(), n_request)
            .expect("sparse complex eigensolve");
            criterion::black_box(lambdas);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_mie_end_to_end_dense,
    bench_mie_end_to_end_sparse
);
criterion_main!(benches);
