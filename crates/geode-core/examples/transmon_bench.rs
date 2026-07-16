//! Reproducible transmon eigenmode **scale-benchmark harness** (Epic #547).
//!
//! This example is the committed, operator-runnable wall-clock gate for the
//! transmon eigensolve. It drives the **exact same** physics and assembly the
//! release cross-check test uses
//! (`tests/transmon_eigenmode.rs::real_transmon_eigenmodes_release`) — the
//! `TransmonPencil` built from the sparse full-tensor Nédélec path, the
//! `L = 14.860 nH ∥ C = 5.5 fF` lumped reactive shunt on the `lumped_element`
//! patch, PEC on metal + exterior, and the real shift-invert Lanczos — and adds
//! only the three things a scaling benchmark needs on top of that fixed
//! formulation:
//!
//!   1. a **mesh-file override** (first positional arg: an external MSH 4.1
//!      mesh, so an operator can point it at the 133k / 1.16M-DOF meshes that
//!      do not live in this repo);
//!   2. an **inner-solver switch** (`GEODE_INNER`), selecting which
//!      `(K − σM)⁻¹` backend the Lanczos shift-invert uses; and
//!   3. **wall-clock timing** of the setup/assembly and solve phases, printed
//!      to stdout so a number can be read straight off a run.
//!
//! Everything else — the eigenvalues, frequencies, and junction participation
//! it prints — is produced by the identical library entry point
//! [`solve_transmon_eigenmodes_with_inner`], so a wall-clock number measured
//! here is directly comparable to the release test's spectrum.
//!
//! # Why this exists
//!
//! Epic #547 is the scale story: direct sparse-LU factorization OOM-kills past
//! a few-hundred-k DOF (~63.9 GB peak at ~1M DOF), while the matrix-free
//! `O(N)`-memory path completes where the direct path dies. This harness is the
//! fixed, reproducible driver used to read those wall-clock numbers off a run
//! at each scale. The headline 1.16M-DOF / 133k timings require an external box
//! and meshes not committed here (an operator/AWS step); locally this harness
//! only needs to build and run the embedded transmon smoke fixture.
//!
//! # Build precondition
//!
//! The `amd` inner path ([`InnerSolver::DirectCustomOrder`], the custom
//! fill-reducing LU ordering) depends on **PR #544** (now merged), which
//! introduced that variant via faer 0.24's public deeper API. This example
//! therefore requires a checkout at or past #544 to compile — it is a hard
//! build precondition, not a runtime toggle.
//!
//! # Usage
//!
//! ```sh
//! cargo build -p geode-core --release --example transmon_bench
//!
//! # embedded smoke fixture, robust direct LU (the default) — always converges:
//! GEODE_INNER=direct GEODE_SIGMA_GHZ=4.5 GEODE_NMODES=6 \
//!     ./target/release/examples/transmon_bench
//!
//! # the 1.16M-DOF scale gate: external mesh, matrix-free CG, timed:
//! GEODE_INNER=matrixfree GEODE_SIGMA_GHZ=4.5 GEODE_NMODES=6 \
//!     /usr/bin/time -v ./target/release/examples/transmon_bench <1.16M.msh>
//! ```
//!
//! # Environment knobs
//!
//! - `GEODE_INNER` — inner `(K − σM)⁻¹` backend (default `direct`, the robust
//!   choice that converges on any shift). One of:
//!   - `direct` → [`InnerSolver::Direct`]: faer COLAMD sparse LU.
//!   - `amd` → [`InnerSolver::DirectCustomOrder`]: custom fill-reducing LU
//!     ordering via faer's public deeper API (**PR #544**; AMD minimum-degree
//!     from the pattern, ~1.4–1.7× less LU fill/memory than COLAMD, same
//!     spectrum).
//!   - `matrixfree` → [`InnerSolver::MatrixFree`]: `O(N)`-memory Jacobi-CG, the
//!     memory-scalable scale path (see the SPD caveat below).
//!   - `minres` → [`InnerSolver::MatrixFreeIndefinite`]: `O(N)`-memory MINRES
//!     for an interior shift where `(K − σM)` is indefinite.
//! - `GEODE_SIGMA_GHZ` — shift frequency in GHz (default `4.5`).
//!
//! # `matrixfree` SPD caveat (why `direct` is the default)
//!
//! `InnerSolver::MatrixFree` is plain preconditioned CG, which requires
//! `(K − σM)` to be **SPD** — i.e. `σ` must sit **below the entire spectrum**.
//! On the ungauged transmon pencil that spectrum includes the `image(d⁰)`
//! gradient nullspace at `λ ≈ 0` **and** a port-localized spurious mode near
//! 3.45 GHz, so a positive shift such as `σ = 4.5 GHz` makes `(K − σM)`
//! indefinite and Jacobi-CG stalls (this is exactly the #526 AMS / #531
//! abs-AMS preconditioner-strength follow-on). The `direct` LU path handles the
//! indefinite shift directly, which is why it is the default for the local
//! smoke run and the ≤few-hundred-k baseline. At the 1.16M-DOF scale where the
//! direct factorization OOMs, run `matrixfree` with `σ` placed strictly below
//! the spectrum (and, once #526's AMS-lite is wired through a public
//! `*_with_gradient` entry point, that preconditioner) — that is the operator
//! wall-clock gate this harness exists to time.
//! - `GEODE_NMODES` — number of eigenmodes to request (default `6`).
//! - `GEODE_NUM_THREADS` — thread count for the faer factorization and the
//!   host-side assembler; honored **internally** by the library
//!   (`geode_core::eigen::parallel::resolve_num_threads`), so no wiring is
//!   needed here — it is echoed below for the record.
//!
//! # Cost-characterization knobs (issue #562, opt-in, matrix-free/`minres` only)
//!
//! These are read **internally** by the shift-invert Lanczos (like
//! `GEODE_NUM_THREADS`), so no wiring is needed here. Each defaults to the exact
//! prior behavior when unset — they exist only to decompose *where the σ=4.5
//! deep-interior MINRES time goes* on the 133k fixture (outer steps vs inner
//! iters vs inner tol) before committing to the 1.16M AWS run:
//!
//! - `GEODE_MINRES_LOG=<k>` — print the inner-MINRES relative preconditioned
//!   residual every `k` iterations (the convergence *curve*; the primary
//!   instrument, since inner-iters/step and inner-tol sensitivity are both read
//!   off it even when the solve does not finish).
//! - `GEODE_EIGEN_STEP_LOG=1` — print each outer Lanczos step's inner iteration
//!   count and cumulative wall-clock (the outer-step count reached in a budget).
//! - `GEODE_INNER_TOL=<x>` — override the absolute inner-MINRES tolerance
//!   (shift-invert tolerates loose inner solves; default is `1e-2 · outer_tol`).
//! - `GEODE_INNER_MAXITERS=<n>` — cap each inner solve so several outer steps
//!   fit a bounded run instead of one solve consuming the whole budget.

use std::time::Instant;

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::lanczos::{InnerPreconditioner, InnerSolver};
use geode_core::eigen::transmon::{
    LumpedReactiveShunt, ModeReport, ReactiveElementNatural, TransmonPencil,
    lambda_shift_for_frequency_hz, solve_transmon_eigenmodes_indefinite_inner_iters_three_space,
    solve_transmon_eigenmodes_with_inner,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{
    TetMesh, TransmonFixture, read_transmon_fixture_from_bytes, read_transmon_smoke_fixture,
};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// The DeviceLayout junction values (issue #492 / mesh::transmon), identical to
/// the release test's constants.
const JUNCTION_L_H: f64 = 14.860e-9;
const JUNCTION_C_F: f64 = 5.5e-15;

/// The DeviceLayout transmon mesh is in **micrometres** (substrate 4 mm ≈ 4000
/// mesh units; see the provenance file).
const M_PER_UNIT: f64 = 1e-6;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Read a `[nnz]` Burn value tensor to a host `Vec<f64>`.
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

/// Assemble the REAL transmon pencil value vectors `(k_vals, m_vals)` via the
/// sparse full-tensor path, asserting the imaginary parts vanish (the pencil is
/// real: PEC + real rotated-sapphire ε + real K_port/M_port). Mirrors the
/// release test's `assemble_real_pencil` exactly.
fn assemble_real_pencil(
    mesh: &TetMesh,
    tet_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_tensor: &[[[c64; 3]; 3]],
) -> (Vec<f64>, Vec<f64>) {
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu_tensor = vec![identity; mesh.n_tets()];

    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        tet_sign,
        scatter,
        epsilon_tensor,
        &nu_tensor,
    );

    let k_re = vals_to_host(sys.k_re_vals);
    let k_im = vals_to_host(sys.k_im_vals);
    let m_re = vals_to_host(sys.m_re_vals);
    let m_im = vals_to_host(sys.m_im_vals);

    let max_k_im = k_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let max_m_im = m_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let scale = k_re
        .iter()
        .chain(m_re.iter())
        .fold(0.0_f64, |a, &b| a.max(b.abs()))
        .max(1.0);
    assert!(
        max_k_im <= 1e-9 * scale,
        "K imaginary part not negligible: {max_k_im} (scale {scale}) — pencil not real"
    );
    assert!(
        max_m_im <= 1e-9 * scale,
        "M imaginary part not negligible: {max_m_im} (scale {scale}) — pencil not real"
    );

    (k_re, m_re)
}

/// Map the `GEODE_INNER` string to an [`InnerSolver`] variant.
fn parse_inner(name: &str) -> Result<InnerSolver, String> {
    match name {
        "direct" => Ok(InnerSolver::Direct),
        // Custom fill-reducing LU ordering via faer's public deeper API (PR #544).
        "amd" => Ok(InnerSolver::DirectCustomOrder),
        "matrixfree" => Ok(InnerSolver::MatrixFree),
        "minres" => Ok(InnerSolver::MatrixFreeIndefinite),
        other => Err(format!(
            "unknown GEODE_INNER='{other}' (expected one of: direct | amd | matrixfree | minres)"
        )),
    }
}

/// Human label for the resolved inner solver (for the stdout banner).
fn inner_label(inner: InnerSolver) -> &'static str {
    match inner {
        InnerSolver::Direct => "direct (faer COLAMD sparse LU)",
        InnerSolver::DirectCustomOrder => "amd (custom-order LU, PR #544)",
        InnerSolver::MatrixFree => "matrixfree (O(N) Jacobi-CG)",
        InnerSolver::MatrixFreeIndefinite => "minres (O(N) indefinite MINRES)",
    }
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    // ---- Resolve the run configuration from args + env. -------------------
    let mesh_arg = std::env::args().nth(1);
    let inner_name = std::env::var("GEODE_INNER").unwrap_or_else(|_| "direct".to_string());
    let inner = match parse_inner(&inner_name) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let sigma_ghz: f64 = env_or("GEODE_SIGMA_GHZ", 4.5_f64);
    let n_modes: usize = env_or("GEODE_NMODES", 6_usize);
    let threads_env = std::env::var("GEODE_NUM_THREADS").unwrap_or_else(|_| "auto".to_string());

    println!("=== transmon_bench (Epic #547 scale-benchmark gate) ===");
    println!(
        "config: inner = {}, σ = {sigma_ghz} GHz, n_modes = {n_modes}, GEODE_NUM_THREADS = {threads_env}",
        inner_label(inner)
    );

    // ---- Phase 1: load the fixture (external mesh override or embedded). ---
    let t_load = Instant::now();
    let f: TransmonFixture = match &mesh_arg {
        Some(path) => {
            println!("mesh: external MSH 4.1 file '{path}'");
            let bytes = std::fs::read(path)
                .unwrap_or_else(|e| panic!("failed to read mesh file '{path}': {e}"));
            read_transmon_fixture_from_bytes(&bytes)
                .unwrap_or_else(|e| panic!("failed to parse transmon fixture '{path}': {e}"))
        }
        None => {
            println!("mesh: embedded transmon smoke fixture (no CLI arg given)");
            read_transmon_smoke_fixture().expect("embedded transmon smoke fixture")
        }
    };
    let load_s = t_load.elapsed().as_secs_f64();
    println!(
        "fixture: {} nodes, {} tets (loaded in {load_s:.3} s)",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    // ---- Phase 2: setup + assembly (identical to the release test path). --
    let t_setup = Instant::now();
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    // PEC on metal + exterior boundary.
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();

    // Real rotated-sapphire ε tensor (lossless).
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let nnz = scatter.nnz();
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    // Junction reactive shunt on the lumped_element patch (DeviceLayout L/C).
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };
    let setup_s = t_setup.elapsed().as_secs_f64();
    println!(
        "setup/assembly: {} edges → {n_interior} interior DOFs, pencil nnz = {nnz} \
         ({setup_s:.3} s)",
        edges.len()
    );

    // ---- Phase 3: shift-invert Lanczos solve (the timed benchmark). -------
    let sigma = lambda_shift_for_frequency_hz(sigma_ghz * 1e9, M_PER_UNIT);
    let t_solve = Instant::now();
    // For the indefinite MINRES path (issues #531/#559) drive the instrumented
    // three-space entry point so we can (a) select the inner preconditioner via
    // `GEODE_PRECOND` (`ams` default — the SPD abs-AMS of #559 — or `jacobi` for
    // the absolute-value-Jacobi baseline) and (b) print the total inner-MINRES
    // iteration count, which is the AMS-vs-abs-Jacobi measurement the acceptance
    // criteria call for. Every other backend keeps the plain entry point.
    let (modes, inner_iters): (Vec<ModeReport>, Option<usize>) =
        if inner == InnerSolver::MatrixFreeIndefinite {
            let precond_name = std::env::var("GEODE_PRECOND").unwrap_or_else(|_| "ams".to_string());
            let precond = match precond_name.as_str() {
                "ams" => InnerPreconditioner::Ams,
                "jacobi" => InnerPreconditioner::Jacobi,
                other => {
                    eprintln!("error: unknown GEODE_PRECOND='{other}' (expected: ams | jacobi)");
                    std::process::exit(2);
                }
            };
            println!(
                "inner preconditioner: {}",
                match precond {
                    InnerPreconditioner::Ams => "three-space AMS (SPD proxy K + |σ|M, #559)",
                    InnerPreconditioner::Jacobi => "absolute-value Jacobi (baseline)",
                }
            );
            let (modes, iters) = solve_transmon_eigenmodes_indefinite_inner_iters_three_space(
                &pencil, sigma, n_modes, M_PER_UNIT, precond,
            )
            .expect("transmon indefinite MINRES eigensolve");
            (modes, Some(iters))
        } else {
            let modes =
                solve_transmon_eigenmodes_with_inner(&pencil, sigma, n_modes, M_PER_UNIT, inner)
                    .expect("transmon eigensolve");
            (modes, None)
        };
    let solve_s = t_solve.elapsed().as_secs_f64();

    // ---- Results + wall-clock (the numbers an operator reads off stdout). -
    println!("eigenvalues / frequencies (sorted by λ):");
    for (i, m) in modes.iter().enumerate() {
        println!(
            "  mode[{i}]: λ = {:.6e}, f = {:.6} GHz, participation p = {:.4}",
            m.lambda,
            m.frequency_ghz(),
            m.participation
        );
    }
    if let Some(iters) = inner_iters {
        println!("total inner-MINRES iterations (summed over outer Lanczos steps): {iters}");
    }
    println!("--- wall-clock ---");
    println!("  fixture-load : {load_s:.3} s");
    println!("  setup+assembly : {setup_s:.3} s");
    println!("  solve ({}) : {solve_s:.3} s", inner_label(inner));
    println!(
        "  TOTAL : {:.3} s ({n_interior} interior DOFs, {n_modes} modes)",
        load_s + setup_s + solve_s
    );
}
