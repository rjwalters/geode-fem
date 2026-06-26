//! Criterion benchmark: dense generalized eigensolve
//! ([`FaerDenseEigensolver::smallest_eigenvalues`]) on the cube
//! Dirichlet K, M pencil.
//!
//! For each `n ∈ {5, 8, 10}` we assemble the system **once**, project
//! it into the interior-node basis, and then time only the eigensolve.
//! `bench_with_input` is used to attach the pre-computed `(K_int, M_int)`
//! pair to each parameter point without rebuilding it per sample.
//!
//! Dense `generalized_eigen` is `O(n³)`; at n=10 the interior pencil
//! is 9³ = 729 — already several seconds per call. The criterion
//! sample budget below is sized to keep total wall-clock manageable.

use std::time::Duration;

use burn::tensor::backend::BackendTypes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::backend::DefaultBackend;
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::cube_tet_mesh;

type B = DefaultBackend;

/// One bench input: pre-computed `(K_int, M_int)` interior pencil.
struct CubePencil {
    k_int: faer::Mat<f64>,
    m_int: faer::Mat<f64>,
}

fn build_pencil(n: usize) -> CubePencil {
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k = burn_matrix_to_faer(sys.k);
    let m = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let (k_int, m_int) =
        apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask).expect("interior projection");
    CubePencil { k_int, m_int }
}

fn bench_dense_eigensolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("eigensolve_dense");
    // n=10 dense `generalized_eigen` takes several seconds per call.
    // 10 samples × measurement_time is the dominant time budget here.
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(20));

    for &n in &[5_usize, 8, 10] {
        // Compute the pencil once per parameter (NOT per sample).
        let pencil = build_pencil(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &pencil, |b, p| {
            b.iter(|| {
                let lambdas = FaerDenseEigensolver
                    .smallest_eigenvalues(p.k_int.as_ref(), p.m_int.as_ref(), 5)
                    .expect("dense eigensolve");
                criterion::black_box(lambdas);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_dense_eigensolve);
criterion_main!(benches);
