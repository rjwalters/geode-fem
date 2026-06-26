//! Criterion benchmark: scalar P1 (`assemble_global_p1`) end-to-end timing
//! on unit-cube tet meshes at several refinement levels.
//!
//! Each sample re-uploads the mesh tensors to the active Burn device
//! (default `wgpu`) as part of the per-sample **setup**; only the
//! assembly call itself is timed. This isolates the autodiff-friendly
//! scatter-add path from the host-side mesh-construction noise.
//!
//! Re-run with:
//! ```sh
//! cargo bench -p geode-core --bench assembly_p1
//! ```
//!
//! Extract a TOML summary of the resulting medians via:
//! ```sh
//! cargo run -p extract_baseline
//! ```

use std::time::Duration;

use burn::tensor::backend::BackendTypes;
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::backend::DefaultBackend;
use geode_core::eigen::dense::burn_matrix_to_faer;
use geode_core::mesh::cube_tet_mesh;

type B = DefaultBackend;

fn bench_assembly_p1(c: &mut Criterion) {
    let device = <B as BackendTypes>::Device::default();
    let mut group = c.benchmark_group("assembly_p1");
    // Keep total wall-clock under the budget — n=10 cube is non-trivial.
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(8));

    for &n in &[5_usize, 8, 10] {
        let mesh = cube_tet_mesh(n, 1.0);
        let n_nodes = mesh.n_nodes();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            // Per-iteration setup uploads the mesh tensors to the device.
            // Timing covers only `assemble_global_p1` itself.
            b.iter_batched(
                || upload_mesh::<B>(&mesh, &device),
                |(nodes_t, tets_t)| {
                    let sys = assemble_global_p1(nodes_t, tets_t, n_nodes);
                    // Force readback so Burn's lazy graph actually
                    // executes the scatter-add kernels rather than just
                    // building IR. This matches every realistic use of
                    // `assemble_global_p1` in this crate (the eigensolve
                    // path always converts to faer before consuming K, M).
                    let k_host = burn_matrix_to_faer(sys.k);
                    criterion::black_box(k_host);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_assembly_p1);
criterion_main!(benches);
