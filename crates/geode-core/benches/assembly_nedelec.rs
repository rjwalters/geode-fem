//! Criterion benchmark: vector Nédélec assembly on cube tet meshes.
//!
//! Two timed routines:
//!
//! - `assemble_global_nedelec` — real-valued K, M assembly.
//! - `assemble_global_nedelec_with_complex_epsilon` — real K plus
//!   `Re(M) + j Im(M)` (two extra scatter passes on the mass), the
//!   Mie-pipeline assembler.
//!
//! Both are exercised at n ∈ {5, 8, 10} cube meshes with vacuum ε = 1+0j.
//! Mesh upload and edge-table construction are done in the **setup** of
//! each per-sample iteration; only the assembly call is timed.

use std::time::Duration;

use burn::tensor::backend::BackendTypes;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use geode_core::{
    assemble_global_nedelec, assemble_global_nedelec_with_complex_epsilon, burn_matrix_to_faer,
    cube_tet_mesh, upload_mesh, DefaultBackend,
};

type B = DefaultBackend;

/// Build the host-side `tet_edge_idx` / `tet_edge_sign` tables from a
/// `TetMesh`. Mirrors the conversion in `examples/mie_sphere.rs`.
fn split_tet_edges(mesh: &geode_core::TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let tet_edges = mesh.tet_edges();
    let idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    (idx, sign)
}

fn bench_assembly_nedelec_real(c: &mut Criterion) {
    let device = <B as BackendTypes>::Device::default();
    let mut group = c.benchmark_group("assembly_nedelec_real");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(8));

    for &n in &[5_usize, 8, 10] {
        let mesh = cube_tet_mesh(n, 1.0);
        let (tet_idx, tet_sign) = split_tet_edges(&mesh);
        let n_edges = mesh.edges().len();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter_batched(
                || upload_mesh::<B>(&mesh, &device),
                |(nodes_t, tets_t)| {
                    let sys =
                        assemble_global_nedelec(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges);
                    let k_host = burn_matrix_to_faer(sys.k);
                    criterion::black_box(k_host);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_assembly_nedelec_complex(c: &mut Criterion) {
    let device = <B as BackendTypes>::Device::default();
    let mut group = c.benchmark_group("assembly_nedelec_complex");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(8));

    for &n in &[5_usize, 8, 10] {
        let mesh = cube_tet_mesh(n, 1.0);
        let (tet_idx, tet_sign) = split_tet_edges(&mesh);
        let n_edges = mesh.edges().len();
        // Vacuum complex ε so the two scatter passes do real work.
        let eps: Vec<faer::c64> = (0..mesh.n_tets())
            .map(|_| faer::c64::new(1.0, 0.0))
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter_batched(
                || upload_mesh::<B>(&mesh, &device),
                |(nodes_t, tets_t)| {
                    let sys = assemble_global_nedelec_with_complex_epsilon(
                        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps,
                    );
                    let k_host = burn_matrix_to_faer(sys.k);
                    criterion::black_box(k_host);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_assembly_nedelec_real,
    bench_assembly_nedelec_complex
);
criterion_main!(benches);
