//! Criterion benchmark: sparse shift-and-invert Lanczos eigensolve on
//! the same cube Dirichlet K, M problem as `eigensolve_dense.rs`.
//!
//! Sigma is pinned at 0.0 (target the smallest end of the spectrum) and
//! the solver requests the lowest 5 eigenvalues, matching the
//! convergence study in `examples/eigen_convergence.rs`.

use std::time::Duration;

use burn::tensor::backend::BackendTypes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use geode_core::{
    DefaultBackend, SparseEigenSolver, SparseShiftInvertLanczos, assemble_global_p1,
    cube_interior_mask, cube_tet_mesh, global_system_to_sparse, upload_mesh,
};

type B = DefaultBackend;

struct SparsePencil {
    sys: geode_core::SparseSystem,
}

fn build_sparse_pencil(n: usize) -> SparsePencil {
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let sparse = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");
    SparsePencil { sys: sparse }
}

fn bench_sparse_eigensolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("eigensolve_sparse");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(8));

    let solver = SparseShiftInvertLanczos::default();

    for &n in &[5_usize, 8, 10] {
        let pencil = build_sparse_pencil(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &pencil, |b, p| {
            b.iter(|| {
                let lambdas = solver
                    .smallest_eigenvalues(p.sys.k.as_ref(), p.sys.m.as_ref(), 5)
                    .expect("sparse eigensolve");
                criterion::black_box(lambdas);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sparse_eigensolve);
criterion_main!(benches);
