//! Spike (issue #451, Epic #374): does the `burn-flex` CPU backend deliver the
//! f64 precision the ARPACK acceptance test and the cube-cavity cross-backend
//! validation rely on to hit a `1e-6` bound against dense oracles?
//!
//! # Why this spike exists
//!
//! Epic #374 proposes migrating `geode-core`'s headless CPU backend from
//! `burn::backend::NdArray<f64, i32>` to `burn::backend::Flex`. The blocker the
//! epic identified — and which this spike verifies against the pinned
//! `burn-flex 0.21.0` crate — is that `Backend` is implemented **only** for the
//! default instantiation `Flex<f32, i32>`. `Flex<f64, i64>` does not satisfy the
//! `Backend` bound (a `compile_fail` doctest locks this in; generic element
//! types are tracked upstream at tracel-ai/burn#4762). So the static associated
//! type `<Flex as BackendTypes>::FloatElem` is *permanently* `f32`, and any f64
//! precision on `flex` must come — if at all — from the **runtime `DType`**
//! machinery, decoupled from the static `FloatElem`.
//!
//! # What this spike measures
//!
//! Three experiments, all comparing against the existing `NdArray<f64, i32>`
//! f64 path on the same tiny cube-cavity assemble + dense generalized
//! eigensolve (`K v = λ M v`, lowest interior Dirichlet mode):
//!
//! 1. **Runtime `DType::F64` feasibility on flex.** Force a `flex` tensor to
//!    `DType::F64` via `Tensor::from_data(data, (&device, DType::F64))` and
//!    confirm (a) the tensor's runtime dtype is genuinely `F64`, and (b) a
//!    round-trip through an op preserves full f64 mantissa. This establishes
//!    whether option **A** (runtime `DType::F64` decoupled from `FloatElem`) is
//!    even physically available on the backend.
//!
//! 2. **The unmodified production path on flex.** Run the *existing* generic
//!    [`upload_mesh`] + [`assemble_global_p1`] on `Flex` with the device's
//!    default float dtype set to `F64` (`set_default_dtypes`), then eigensolve.
//!    This is the "naive swap" scenario. It exposes the load-bearing precision
//!    leak: [`upload_mesh`] materializes `Vec<B::FloatElem>` = `Vec<f32>` on
//!    flex and casts every node coordinate through `.elem::<f32>()` **before**
//!    the F64 tensor is built, so double precision is lost at the host boundary
//!    even though the on-device tensor is nominally `F64`.
//!
//! 3. **The option-A recipe, applied.** A spike-local f64-forced upload that
//!    keeps `Vec<f64>` end-to-end (`from_data(.., (&device, DType::F64))`)
//!    instead of `Vec<B::FloatElem>`, feeding the *same* generic
//!    [`assemble_global_p1`]. This demonstrates that once the readback/upload
//!    sites are converted off the static `FloatElem` (exactly option A's
//!    prescription), flex recovers f64 parity with ndarray.
//!
//! Run it:
//!
//! ```bash
//! cargo run -p geode-core --example flex_precision_spike \
//!     --no-default-features --features flex --release
//! ```
//!
//! `--release` matters: faer 0.24's real QZ path trips a debug-assertion in
//! debug builds (same reason the `cube_convergence` regression test is
//! `#[ignore]`d outside release).
//!
//! This file is additive scratch. It touches no production precision-threading
//! site; it only *calls* the existing generic assembly/eigensolve entry points.

use burn::prelude::Backend;
use burn::tensor::backend::BackendTypes;
use burn::tensor::set_default_dtypes;
use burn::tensor::{DType, FloatDType, Int, IntDType, Tensor, TensorData};

use burn::backend::Flex;
use burn::backend::NdArray;

use geode_core::assembly::p1::{GlobalSystem, assemble_global_p1, upload_mesh};
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

/// The ndarray reference backend — the current production CPU backend,
/// statically f64 (`FloatElem = f64`).
type Ndf64 = NdArray<f64, i32>;

/// Cube subdivisions for the parity problem. Small enough for the dense
/// generalized eigensolve to run in well under a second, large enough that
/// f32-vs-f64 assembly differences show up above f64 round-off.
const CUBE_N: usize = 4;

/// Cube side length. Deliberately **non-dyadic** (`1/3`) so the node
/// coordinates are NOT exactly representable in f32. A dyadic side (e.g.
/// `1.0`) places nodes at `k/n` = 0, 0.25, 0.5, 0.75, 1.0 — all f32-exact —
/// which would make an f32 upload lossless *for that mesh* and hide the very
/// precision leak this spike is measuring. `1/3` forces every interior node
/// coordinate off the f32 grid so the leak becomes visible in the eigenvalue.
const CUBE_SIDE: f64 = 1.0 / 3.0;

/// The f64 parity bar the cube-cavity oracle relies on.
const PARITY_TOL: f64 = 1e-6;

fn main() {
    println!("=== flex f64-precision spike (issue #451 / Epic #374) ===\n");

    // The static FloatElem facts, printed so the evidence is self-contained.
    print_static_facts();

    // Reference: the ndarray f64 ground mode.
    let mesh = cube_tet_mesh(CUBE_N, CUBE_SIDE);

    // Diagnostic escape hatch: `TRACE_EXP3=1` runs experiment 3 uncaught so its
    // panic backtrace (which pinpoints the f32-hardcoded constant in the
    // element kernel) is visible. Off by default.
    if std::env::var("TRACE_EXP3").is_ok() {
        eprintln!(
            "[TRACE_EXP3] running experiment 3 FIRST (device un-initialized) to \
             show it succeeds when set_default_dtypes(F64) wins the one-shot latch..."
        );
        let lambda_ref = ndarray_ground_mode(&mesh);
        let lambda = experiment_option_a(&mesh);
        let rel = rel_err(lambda, lambda_ref);
        eprintln!(
            "[TRACE_EXP3] λ0 = {lambda:.15e}  ref = {lambda_ref:.15e}  rel = {rel:.3e}  {}",
            verdict(rel)
        );
        return;
    }

    let lambda_ref = ndarray_ground_mode(&mesh);
    println!(
        "reference  NdArray<f64,i32>  λ0 = {lambda_ref:.15e}   (n_nodes = {})\n",
        mesh.n_nodes()
    );

    // Experiment 1: is runtime DType::F64 even available on flex?
    let dtype_ok = experiment_runtime_f64_dtype();

    // Experiment 2: the true naive swap — unmodified production path on flex
    // with the *default* device dtype policy (f32, the static FloatElem). No
    // `set_default_dtypes`. This is what a mechanical `NdArray -> Flex` type
    // swap gets you today. Run BEFORE experiment 3, since `set_default_dtypes`
    // is a one-shot per-device latch. Captured because it may panic.
    let naive = run_capturing("experiment 2 (naive flex swap, default f32)", || {
        experiment_naive_flex(&mesh)
    });

    // Experiment 3: option-A recipe — `set_default_dtypes(F64)` PLUS an
    // f64-forced upload feeding the same generic assemble_global_p1. Captured
    // for symmetry.
    //
    // KEY ORDERING CAVEAT: experiment 2 above already created f32 tensors on
    // the shared `FlexDevice`, which LATCHED the device's dtype policy to f32.
    // `set_default_dtypes` is a one-shot-per-device init, so experiment 3's
    // `set_default_dtypes(F64)` here returns `AlreadyInitialized` (ignored) and
    // the internal `from_floats`/`zeros` accumulators in `assemble_global_p1`
    // stay f32 — colliding with the F64 nodes tensor and panicking. Run this
    // spike with `TRACE_EXP3=1` to see experiment 3 run FIRST (winning the
    // latch) and hit EXACT f64 parity (rel = 0). That split is the finding:
    // option A works only if F64 wins the device latch before ANY f32 tensor
    // exists in the process — a real fragility for a shared test binary.
    let opt_a = run_capturing("experiment 3 (option A f64-forced upload)", || {
        experiment_option_a(&mesh)
    });

    println!("\n=== parity summary vs NdArray<f64,i32> ===");
    println!("  experiment 1 (runtime DType::F64 available on flex): {dtype_ok}");
    report_experiment(
        "experiment 2 (naive swap, Vec<B::FloatElem>=Vec<f32> upload)",
        naive,
        lambda_ref,
    );
    report_experiment(
        "experiment 3 (option A, Vec<f64> forced upload)          ",
        opt_a,
        lambda_ref,
    );

    println!("\n=== conclusion ===");
    println!(
        "  Static Flex::FloatElem is f32 and CANNOT change (Backend impl is\n\
         \x20 Flex<f32,i32>-only; Flex<f64,i64>: Backend is a compile_fail). So\n\
         \x20 any f64 on flex must be runtime-DType (option A). Findings:\n\
         \n\
         \x20 (1) Runtime DType::F64 IS available per-tensor (exp 1): from_data\n\
         \x20     with (&device, DType::F64) yields a genuine F64 tensor that\n\
         \x20     preserves f64 mantissa through ops and iter::<f64>() readback.\n\
         \n\
         \x20 (2) Naive swap = silent f32 (exp 2): a mechanical NdArray->Flex\n\
         \x20     type swap with the default device policy runs the ENTIRE\n\
         \x20     assemble+eigensolve at f32 (nodes tensor dtype = F32). It does\n\
         \x20     NOT panic — it just downgrades. On this 1/3-sided cube it lands\n\
         \x20     ~1.3e-7 off the f64 reference; on the real ARPACK / cube-cavity\n\
         \x20     oracles that f32 error would not survive the 1e-6 bound.\n\
         \n\
         \x20 (3) Option A reaches EXACT f64 parity (rel = 0) — but only when:\n\
         \x20     (a) set_default_dtypes(F64) wins the device's ONE-SHOT init\n\
         \x20         latch, i.e. runs before ANY f32 tensor exists on the\n\
         \x20         process's shared FlexDevice (run TRACE_EXP3=1 to see this\n\
         \x20         path succeed); AND\n\
         \x20     (b) every upload/readback site is converted off B::FloatElem to\n\
         \x20         explicit Vec<f64> + DType::F64.\n\
         \x20     If f32 wins the latch first (as exp 2 makes it), option A\n\
         \x20     PANICS: flex binary_op does NOT promote mixed F64xF32, and the\n\
         \x20     f32-HARDCODED constant in the element kernel\n\
         \x20     (elements/p1.rs:135, Tensor::from_floats -> .convert::<f32>())\n\
         \x20     collides with the F64 field, aborting inside FlexTensor::storage\n\
         \x20     ('dtype mismatch expected F32, got F64').\n\
         \n\
         \x20 Net: option A is viable but INVASIVE and FRAGILE. Cost is larger\n\
         \x20 than the epic's cited readback anchors: it also requires replacing\n\
         \x20 f32-hardcoded constant constructors (from_floats) in the kernels,\n\
         \x20 a whole-device F64 latch ordered before all tensor creation, and a\n\
         \x20 dtype-uniformity audit of every op — whereas ndarray delivers the\n\
         \x20 same f64 statically for free. See the #374 decision comment."
    );
}

/// Run an experiment closure, capturing any panic (a `flex` dtype-mismatch
/// panic is itself decision-relevant evidence) and returning either the
/// eigenvalue or the panic message.
fn run_capturing<F: FnOnce() -> f64 + std::panic::UnwindSafe>(
    label: &str,
    f: F,
) -> Result<f64, String> {
    // Suppress the default panic hook's noisy backtrace for the captured runs;
    // we print our own tidy diagnostic instead.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(f);
    std::panic::set_hook(prev);

    match out {
        Ok(lambda) => Ok(lambda),
        Err(e) => {
            let msg = e
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            println!("  {label} PANICKED: {msg}");
            Err(msg)
        }
    }
}

/// Print a parity line for a captured experiment.
fn report_experiment(label: &str, outcome: Result<f64, String>, lambda_ref: f64) {
    match outcome {
        Ok(lambda) => {
            let rel = rel_err(lambda, lambda_ref);
            println!(
                "  {label}: λ0 = {lambda:.15e}  rel = {rel:.3e}  {}",
                verdict(rel)
            );
        }
        Err(msg) => {
            println!("  {label}: PANIC — {msg}");
        }
    }
}

/// Print the immutable static-type facts the decision hinges on.
fn print_static_facts() {
    let flex_float = core::mem::size_of::<<Flex as BackendTypes>::FloatElem>();
    let nd_float = core::mem::size_of::<<Ndf64 as BackendTypes>::FloatElem>();
    println!("static backend facts:");
    println!(
        "  size_of::<Flex::FloatElem>()          = {flex_float} bytes  \
         ({})",
        if flex_float == 4 {
            "f32 — CANNOT be f64"
        } else {
            "unexpected"
        }
    );
    println!(
        "  size_of::<NdArray<f64,i32>::FloatElem>() = {nd_float} bytes  \
         ({})\n",
        if nd_float == 8 { "f64" } else { "unexpected" }
    );
}

/// Experiment 1: confirm a `flex` tensor can be created at runtime `DType::F64`
/// and preserves f64 mantissa through an op. Returns whether both held.
fn experiment_runtime_f64_dtype() -> bool {
    let device = <Flex as BackendTypes>::Device::default();

    // A value that is NOT representable in f32 without loss: 1 + 2^-40.
    // In f32 this rounds to exactly 1.0; in f64 it is retained.
    let v: f64 = 1.0 + 2f64.powi(-40);

    // Force DType::F64 explicitly, decoupled from the static FloatElem (f32).
    let t =
        Tensor::<Flex, 1>::from_data(TensorData::new(vec![v, v, v], [3]), (&device, DType::F64));

    let runtime_dtype = t.dtype();
    let dtype_is_f64 = runtime_dtype == DType::F64;

    // Round-trip through an op and read back losslessly via the dtype-aware
    // `iter::<f64>()` path (same one `burn_matrix_to_faer` uses — NOT
    // `to_vec::<E>()`, which requires an exact element-type match).
    //
    // IMPORTANT — spike finding: the *other* operand MUST also be forced to
    // `DType::F64`. `Tensor::ones([3], &device)` builds an f32 tensor (the
    // static-`FloatElem` device default), and `flex`'s `binary_op` does NOT
    // promote a mixed F64×F32 pair — it panics inside `FlexTensor::storage`
    // with "dtype mismatch (expected F32, got F64)". This is the same class of
    // failure a naive swap hits (see experiment 2). Ops must keep BOTH operands
    // at F64 for the runtime-DType strategy to hold.
    let ones_f64 = Tensor::<Flex, 1>::from_data(
        TensorData::new(vec![1.0f64, 1.0, 1.0], [3]),
        (&device, DType::F64),
    );
    let doubled = t.clone() * ones_f64;
    let read: Vec<f64> = doubled.into_data().iter::<f64>().collect();
    let preserved = (read[0] - v).abs() < 1e-15;

    println!("experiment 1 — runtime DType::F64 on flex:");
    println!("  requested dtype           : F64");
    println!("  tensor.dtype()            : {runtime_dtype:?}  (== F64? {dtype_is_f64})");
    println!(
        "  probe value 1+2^-40       : stored={v:.17}  read_back={:.17}",
        read[0]
    );
    println!(
        "  f64 mantissa preserved    : {preserved}  (f32 would round to 1.0, \
         error {:.3e})\n",
        (1.0f32 as f64 - v).abs()
    );

    dtype_is_f64 && preserved
}

/// Experiment 2: the naive `flex` swap. Run the *unmodified* generic
/// `upload_mesh` + `assemble_global_p1` on flex with the device's DEFAULT dtype
/// policy — which is f32 (the static `FloatElem`). No `set_default_dtypes`.
///
/// This is precisely what a mechanical `NdArray<f64,i32> -> Flex` type swap
/// yields today: `upload_mesh` builds `Vec<B::FloatElem>` = `Vec<f32>` and casts
/// every node coordinate through `.elem::<f32>()`, and the internal
/// `zeros`/scatter accumulators are f32 too. The whole pipeline silently runs at
/// f32 — no panic, just a downgraded result. The eigenvalue therefore drifts
/// from the f64 reference by ~f32 round-off (visible because `CUBE_SIDE = 1/3`
/// puts the node coordinates off the f32-exact grid).
fn experiment_naive_flex(mesh: &TetMesh) -> f64 {
    let device = <Flex as BackendTypes>::Device::default();

    let (nodes, tets) = upload_mesh::<Flex>(mesh, &device);
    println!("experiment 2 — naive swap on flex (DEFAULT dtype policy):");
    println!(
        "  nodes tensor dtype        : {:?}  (device default = static FloatElem = f32)",
        nodes.dtype()
    );
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    ground_mode_from_system(sys, mesh)
}

/// Experiment 3: option A applied. `set_default_dtypes(F64)` makes every
/// internal `zeros`/`ones`/scatter accumulator F64 (so no mixed-dtype op
/// panics), AND the upload keeps `Vec<f64>` end-to-end forcing `DType::F64` at
/// the tensor boundary (so node coordinates are not f32-truncated at the host
/// cast). Both halves are required — dropping either reproduces experiment 2's
/// f32 downgrade or a mixed-dtype panic. This is exactly what "convert every
/// `B::FloatElem` upload/readback site to explicit f64 and audit every internal
/// accumulator dtype" prescribes.
fn experiment_option_a(mesh: &TetMesh) -> f64 {
    let device = <Flex as BackendTypes>::Device::default();
    // One-shot per-device latch to F64. Idempotent for our purposes: if a prior
    // run already set it, `AlreadyInitialized` is fine.
    let _ = set_default_dtypes::<Flex>(&device, FloatDType::F64, IntDType::I32);

    let (nodes, tets) = upload_mesh_f64_forced(mesh, &device);
    println!("experiment 3 — option A on flex (set_default_dtypes(F64) + f64-forced upload):");
    println!("  nodes tensor dtype        : {:?}", nodes.dtype());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    ground_mode_from_system(sys, mesh)
}

/// Option-A upload: identical shape to `upload_mesh`, but keeps the node
/// coordinates in `Vec<f64>` and forces `DType::F64` rather than routing
/// through the static `B::FloatElem` (which is f32 on flex). Node connectivity
/// stays `i32` exactly as production does.
fn upload_mesh_f64_forced(
    mesh: &TetMesh,
    device: &<Flex as BackendTypes>::Device,
) -> (Tensor<Flex, 2>, Tensor<Flex, 2, Int>) {
    let n_nodes = mesh.n_nodes();
    let n_elem = mesh.n_tets();

    // The critical difference from production `upload_mesh`: Vec<f64>, not
    // Vec<B::FloatElem>. No lossy `.elem::<f32>()` host cast.
    let node_data: Vec<f64> = mesh.nodes.iter().flat_map(|n| n.iter().copied()).collect();
    let nodes = Tensor::<Flex, 2>::from_data(
        TensorData::new(node_data, [n_nodes, 3]),
        (device, DType::F64),
    );

    let tet_data: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| {
            t.iter()
                .map(|&i| i32::try_from(i).expect("node index fits i32"))
        })
        .collect();
    let tets = Tensor::<Flex, 2, Int>::from_data(TensorData::new(tet_data, [n_elem, 4]), device);

    (nodes, tets)
}

/// The ndarray f64 reference: unmodified production path.
fn ndarray_ground_mode(mesh: &TetMesh) -> f64 {
    let device = <Ndf64 as BackendTypes>::Device::default();
    let (nodes, tets) = upload_mesh::<Ndf64>(mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    ground_mode_from_system(sys, mesh)
}

/// Shared tail: reduce with Dirichlet BC and dense-eigensolve the lowest mode.
/// `burn_matrix_to_faer` reads via the dtype-aware `iter::<f64>()`, so it is
/// lossless for both the ndarray-f64 and flex-F64 tensors.
fn ground_mode_from_system<B: Backend>(sys: GlobalSystem<B>, mesh: &TetMesh) -> f64 {
    let k = burn_matrix_to_faer(sys.k);
    let m = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, CUBE_SIDE);
    let (k_int, m_int) = apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask).expect("BC reduction");
    FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), 1)
        .expect("eigensolve")[0]
}

fn rel_err(got: f64, reference: f64) -> f64 {
    (got - reference).abs() / reference.abs().max(1.0)
}

fn verdict(rel: f64) -> &'static str {
    // Distinguish EXACT f64 agreement (bit-for-bit / round-off-only) from
    // "merely under 1e-6". The naive f32 path can squeak under 1e-6 on this
    // tiny well-conditioned problem while still being genuine f32 error — not
    // the f64 parity the oracle needs on harder meshes.
    if rel < 1e-12 {
        "EXACT f64 parity (rel < 1e-12)"
    } else if rel < PARITY_TOL {
        "< 1e-6 but f32-level error (NOT f64 parity)"
    } else {
        "> 1e-6 (fails f64 oracle bar)"
    }
}
