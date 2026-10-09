//! Mie-sphere benchmark — FEM eigenmodes vs. analytic PEC-cavity
//! dielectric-sphere resonance roots (issue #4, north-star deliverable).
//!
//! This is the Epic #398 **pilot example crate**: a standalone binary
//! (`examples/mie_sphere/`) built on the `geode-app` harness, migrated
//! from the old `crates/geode-core/examples/mie_sphere.rs`. The physics,
//! report output, and `results.toml`/`.vtu` artifacts are preserved
//! exactly; only the entry point (hand-rolled argv → `clap` derive +
//! `geode_app::App`) and the viz-reconstruction import
//! (`#[path]` include → `geode_util::viz`) changed.
//!
//! **v1** (issue #40 hardening of the v0 in PR #39):
//!
//! - **Extended analytic catalog**: roots for `l ∈ [1, L_MAX]`, both TE
//!   and TM polarisations, lowest `N_MAX` radial overtones each — about
//!   40 entries in the [0.1, 20] `k` window. Computed via the same
//!   Newton+bisection scheme as v0, with Miller's downward recurrence
//!   for the spherical Bessel `j_l` at high `l` / small `x`.
//! - **Multiplicity-aware pairing**: labels each FEM mode with an
//!   analytic `(pol, l, n)` and its slot `m_idx ∈ [0, 2l]` within the
//!   magnetic-degeneracy multiplet. v0 used nearest-`k` pairing, which
//!   mislabelled the second FEM triplet as TM_1,1. v1 claimed `2 l + 1`
//!   consecutive FEM modes per root in ascending analytic `k`. Issue
//!   #1000 replaced that with cluster pairing, which splits the FEM
//!   spectrum into multiplets by complex-plane scatter and matches each
//!   cluster to a root by size, because after #986 the analytic `k` order
//!   of TM_2,1 / TE_1,1 differs from the UPML pencil's order.
//! - **Im(k) banding sanity check**: within a claimed multiplet, the
//!   per-mode Q's should be within ~10 % of each other. We log
//!   violations as informational notes (mesh asymmetry routinely
//!   breaks the band on the bundled fixture).
//!
//! # Honest scope
//!
//! - **Analytic side**: real-only PEC-cavity roots are the primary
//!   pairing target (multiplicity-claim logic below). The open-space
//!   Mie WGM positions (complex `k`, outgoing-wave BC) are also
//!   tabulated in `geode_core::analytic::mie::OPEN_SPACE_WGM_TABLE_N15` (issue #33)
//!   and printed as a side-by-side cross-check at the bottom of the
//!   run — they are the physically correct ground truth, but the
//!   PML-truncated FEM does not yet reach them tightly (~30–40 % rel
//!   err on `Re(k)` at the bundled fixture). Tightening that gap is
//!   the target of #35.
//! - **FEM side**: 774-node tet mesh (the bundled refined fixture
//!   from issue #49, bumped from the original 313 nodes), **anisotropic
//!   UPML** (diagonal complex permittivity tensor, issue #54) over the
//!   vacuum buffer, σ₀ = 5.0, k₀_ref = 2.0. Expect ≈ 3.6 % relative
//!   error in `Re(k)` for the lowest TM_1,1 mode. The legacy
//!   scalar-isotropic PML (~16 % rel err) is still available via
//!   `--scalar-pml`; see comments in `tests/mie_sphere.rs`.
//! - **Driven scattering** (Q_ext, Q_sca vs. ka) remains v2.
//!
//! Quantitative tightening lives in follow-up issues (#33, #35, #38).
//!
//! # Running
//!
//! ```sh
//! cargo run -p mie_sphere --release
//! ```
//!
//! By default the **sparse complex shift-and-invert Lanczos**
//! eigensolver (issue #53) runs the FEM eigenproblem against the
//! anisotropic UPML kernel (issue #54). Pass `--dense` to fall back
//! on the dense `FaerComplexEigensolver` (the correctness oracle),
//! and/or `--scalar-pml` to use the legacy scalar-isotropic PML for
//! the cross-check baseline:
//!
//! ```sh
//! cargo run -p mie_sphere --release -- --dense
//! cargo run -p mie_sphere --release -- --scalar-pml
//! ```
//!
//! `--release` is required because faer 0.24's `gevd` path panics
//! under `debug-assertions` (same root cause as `tests/sphere_pml_*`).
//! The sparse path is independent of `gevd` but the dense fallback
//! still needs release mode.
//!
//! Writes `benchmarks/mie_sphere/results.toml` relative to the
//! workspace root (located via `CARGO_MANIFEST_DIR`).
//!
//! # Field export (Epic #276 Phase 2B, issue #287)
//!
//! Passing `--export-field` is an opt-in side channel that does **not**
//! touch the eigenmode benchmark above (the `results.toml` is
//! byte-identical with or without it). When present, the bundled
//! sphere is solved once as a *driven scattering* problem — a plane
//! wave `E_inc = x̂·exp(−iωz)` illuminating the `n = 1.5` dielectric
//! sphere via the matched (full Sacks) UPML scattered-field solve
//! (`geode_core::driven::scattering::solve_scattered_field_matched_upml`, the same machinery
//! as the `mie_driven_scattering` example) — and the scattered near
//! field `E_sca(r)` is dumped to `<out-dir>/E_mie.vtu` (the `--out-dir`
//! group from `geode-app`, default `artifacts/`) for ParaView inspection:
//!
//! ```sh
//! cargo run -p mie_sphere --release -- --export-field
//! cargo run -p mie_sphere --release -- --export-field --out-dir artifacts/viz
//! ```
//!
//! The default frequency is the **mid-`ka` point** of the driven
//! benchmark sweep (`ka = 1.9`, between the TE_1,1 and TM_1,1 Mie
//! resonances; `ω = ka / R_SPHERE`). The exported per-node `E` is the
//! crude per-tet-vertex average of the Whitney interpolant (see
//! `geode_util::viz::edge_field_to_nodes`); a per-node `eps_r`
//! map (n² inside the sphere, 1 outside) accompanies it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use burn::prelude::Backend;
use burn::tensor::backend::BackendTypes;
use clap::Parser;
use faer::sparse::{SparseColMat, Triplet};

use geode_app::{App, OutputDir, Verbosity};
use geode_core::analytic::mie::{MieRoot, mie_roots_catalog, open_space_wgm_roots_n15};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_anisotropic_epsilon, assemble_global_nedelec_with_complex_epsilon,
    build_anisotropic_pml_tensor_diag, build_complex_epsilon_r_pml, burn_complex_mass_to_faer,
    sphere_n_interior_nodes, sphere_pec_interior_edges, tet_centroid_radii, tet_centroids,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::driven::scattering::{
    plane_wave_polarization_current, solve_scattered_field_matched_upml,
};
use geode_core::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{PHYS_SPHERE_INTERIOR, R_BUFFER, R_SPHERE, read_sphere_fixture};
use geode_core::testing::TestBackend;
use geode_util::eigen;
use geode_util::fixture::BackendInfo;
use geode_util::viz::edge_field_to_nodes;

/// Refractive index inside the sphere. n=1.5 is the textbook
/// B&H dielectric test case.
const N_INSIDE: f64 = 1.5;

/// PML absorption strength. σ₀ = 5.0 matches `tests/sphere_pml_*`.
const SIGMA_0: f64 = 5.0;

/// Reference wavenumber used to scale the anisotropic-UPML stretching
/// profiles `s_r = s_t = 1 - jσ/(ω₀ ε₀)` with ω₀ = k₀_ref. Matches the
/// `tests/sphere_pml_anisotropic_eigenmode.rs` acceptance test —
/// k₀_ref ≈ 2.0 is near the lowest physical mode's `Re(k)` and gives
/// the documented ≈ 3.6% TM_1,1 rel err on the bundled fixture
/// (against the root corrected in issue #986; it read ~6% against the
/// old root).
const K0_REF: f64 = 2.0;

/// Real shift `σ` for the sparse shift-and-invert Lanczos default path
/// (issue #691).
///
/// Must be **strictly positive**: the reduced Nédélec stiffness `K` has
/// an exact discrete-gradient null space (dimension = interior node
/// count, 368 on the bundled fixture), so `K − 0·M` is singular. The
/// historical `σ = 0` shift happened to yield plausible low modes on the
/// `bef2d7d` wgpu-f32 build that produced the pre-#691 artifact, but on
/// current `main` it collapses on **both** f32 and f64 NdArray: the
/// numerically singular LU drives every Ritz value into the null cluster
/// (`|λ| ~ 1e-14` on f64) and no physical mode is extracted. Placing `σ`
/// between the null cluster (`λ = 0`) and the lowest physical
/// `k² ≈ 1.5` makes `K − σM` non-singular and collapses the whole null
/// space onto the single mapped eigenvalue `μ = −1/σ`. The extracted
/// physical spectrum is insensitive to the exact choice (σ = 1.0 and
/// σ = 2.5 agree to 5 significant figures on every mode).
const LANCZOS_SIGMA: f64 = 1.0;

/// Spurious-mode cutoff relative to [`LANCZOS_SIGMA`]: returned `|λ| <
/// NULL_TOL_REL · σ` is gradient-null-space noise (issue #696). Mirrors
/// `PecCavitySettings::null_tol_rel · σ` (#685).
const NULL_TOL_REL: f64 = 0.5;

/// Number of analytic multiplets to walk when sizing the FEM eigen
/// request (issue #43).
///
/// `N_MODES` (the actual count of FEM modes requested above the
/// gradient nullspace) is no longer a hand-tuned magic number — it is
/// derived from the cumulative multiplicities of the first
/// `N_ANALYTIC_GROUPS` rows in `mie_roots_catalog`. With the n = 1.5,
/// `L_MAX = 4`, `N_MAX = 5` catalog the first three groups are
/// `TM_1,1` (multiplicity 3, k ≈ 1.18710), `TM_2,1` (multiplicity 5,
/// k ≈ 1.81333), and `TE_1,1` (multiplicity 3, k ≈ 1.86880), summing
/// to 11 — comfortably above the v1 hand-set count of 8 and including
/// the `TM_2,1` / `TE_1,1` close-pair (3.06 % spacing on this catalog)
/// that exercises the cluster pairing (issue #1000). (Before issue #986
/// corrected the PEC-cavity roots, the order was TM_1,1 / TE_1,1 /
/// TM_2,1 with a 0.07 % TE_1,1 / TM_2,1 gap.) Mesh refinement or
/// catalog widening (`L_MAX`, `N_MAX`) automatically tracks through
/// this derivation.
const N_ANALYTIC_GROUPS: usize = 3;

/// Maximum angular order in the analytic catalog (issue #40).
const L_MAX: usize = 4;
/// Maximum radial order per `(l, pol)` in the analytic catalog.
const N_MAX: usize = 5;

/// Relative-gap threshold (~10 %) for flagging a close-pair in the
/// analytic catalog (issue #43). Two roots within this fraction of each
/// other are a close pair. Their rows are flagged ambiguous only when the
/// pairing cannot separate them cleanly; see [`pair_modes`] (issue #1000).
const CLOSE_PAIR_GAP_FRAC: f64 = 0.10;

/// Compute the requested FEM-mode count from the first
/// `N_ANALYTIC_GROUPS` rows of the catalog. Sums their multiplicities
/// so refinement-driven changes (more rows, different multiplicities)
/// track automatically.
fn n_modes_from_catalog(analytic: &[MieRoot]) -> usize {
    analytic
        .iter()
        .take(N_ANALYTIC_GROUPS)
        .map(|r| r.multiplicity)
        .sum()
}

/// Result for a single benchmark row.
///
/// Each row records one FEM eigenmode together with the analytic
/// `(l, n, polarisation)` group it was claimed into and which slot
/// within the `2 l + 1` magnetic-degeneracy multiplet it occupies.
#[derive(Debug, Clone, serde::Serialize)]
struct Row {
    #[serde(rename = "polarisation")]
    pol: &'static str,
    l: usize,
    n: usize,
    /// Slot within the `2 l + 1` degenerate group (0-indexed).
    m_idx: usize,
    /// True if the FEM spectrum ran out before the full multiplicity
    /// was filled.
    incomplete: bool,
    /// True if another paired analytic root lies within
    /// `CLOSE_PAIR_GAP_FRAC` of this one and the pairing could not
    /// separate them cleanly (issues #43, #1000). For cluster pairing,
    /// that means either the FEM multiplet segmentation is not decisive,
    /// or the close neighbour has the same multiplicity. For the
    /// ascending-`k` fallback, it means the claimed FEM modes span more
    /// than half the inter-root gap. Treat the row's `(pol, l, n)` label
    /// as a best guess rather than physically certain.
    ambiguous: bool,
    analytic_k: f64,
    fem_re_k: f64,
    fem_im_k: f64,
    rel_err_re_k: f64,
    q: f64,
}

fn results_path() -> PathBuf {
    // `examples/mie_sphere/src/main.rs` → `CARGO_MANIFEST_DIR` is
    // `examples/mie_sphere`; walk up 2 levels to the workspace root, then
    // into `benchmarks/mie_sphere/` (same level count as the old
    // `crates/geode-core` manifest dir).
    geode_util::repo::repo_root()
        .join("benchmarks")
        .join("mie_sphere")
        .join("results.toml")
}

/// Run the FEM eigensolve and return the lowest `N_MODES` physical
/// eigenvalues `(k², Q)` as `Complex<f64>`s in `k = sqrt(λ)`.
///
/// `use_dense` selects the eigensolver: `true` uses the dense
/// `FaerComplexEigensolver` (the correctness oracle), `false` uses the
/// sparse `SparseComplexShiftInvertLanczos` (the default fast path).
///
/// `scalar_pml` selects the PML kernel: `true` uses the legacy
/// scalar-isotropic complex ε (16% rel err ceiling, issue #52),
/// `false` (default) uses the anisotropic-UPML diagonal complex
/// tensor (≈ 3.6% rel err on TM_1,1, issue #54).
fn fem_complex_k<B: Backend>(
    device: &B::Device,
    use_dense: bool,
    scalar_pml: bool,
    n_modes: usize,
) -> Vec<faer::c64> {
    let f = read_sphere_fixture().expect("fixture load");
    eprintln!(
        "sphere fixture: {} nodes, {} tets, {} boundary triangles",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.boundary_triangles.len(),
    );

    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges_idx = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, device);
    let sys = if scalar_pml {
        let radii = tet_centroid_radii(&f.mesh);
        let eps_complex =
            build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);
        eprintln!(
            "PML kernel: scalar-isotropic complex ε (legacy, --scalar-pml; expect ~16% TM_1,1 rel err)"
        );
        assemble_global_nedelec_with_complex_epsilon(
            nodes_t,
            tets_t,
            &tet_idx,
            &tet_sign,
            n_edges,
            &eps_complex,
        )
    } else {
        let centroids = tet_centroids(&f.mesh);
        let eps_aniso = build_anisotropic_pml_tensor_diag(
            &f.tet_physical_tags,
            &centroids,
            N_INSIDE,
            SIGMA_0,
            K0_REF,
        );
        eprintln!(
            "PML kernel: anisotropic UPML diagonal complex ε (default, issue #54; k₀_ref = {K0_REF}; expect ≈ 3.6% TM_1,1 rel err)"
        );
        assemble_global_nedelec_with_anisotropic_epsilon(
            nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
        )
    };

    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("BC reduction K");

    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    eprintln!(
        "FEM matrix size after PEC reduction: {} × {} (complex)",
        dim, dim
    );

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + n_modes + 5;

    eprintln!("predicted spurious-mode count: {spurious_dim}, requesting {n_request} eigenvalues",);

    let t_solve = std::time::Instant::now();
    let lambdas = if use_dense {
        eprintln!("eigensolver: dense FaerComplexEigensolver (oracle)");
        FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(
                k_int_complex.as_ref(),
                m_int_complex.as_ref(),
                n_request,
            )
            .expect("dense complex eigensolve")
    } else {
        eprintln!("eigensolver: sparse SparseComplexShiftInvertLanczos (default)");
        // Project the dense complex matrices into sparse CSC form.
        // The Mie pencil's Nédélec stencil is genuinely sparse — we
        // walk the dense entries and keep only the non-zeros. At the
        // bundled-fixture size (a few hundred interior edges) the
        // cost of this pass is negligible next to the dense oracle's
        // full-spectrum solve, but for the larger refined meshes it
        // pays for itself many times over.
        let n = k_int_complex.nrows();
        let mut k_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
        let mut m_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
        for j in 0..n {
            for i in 0..n {
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
        let k_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(n, n, &k_trips)
            .expect("complex K sparsification");
        let m_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(n, n, &m_trips)
            .expect("complex M sparsification");
        eprintln!(
            "  sparsified pencil: nnz(K) = {}, nnz(M) = {}",
            k_trips.len(),
            m_trips.len()
        );
        SparseComplexShiftInvertLanczos {
            sigma: LANCZOS_SIGMA,
            max_iters: 256,
            tol: 1e-9,
        }
        .smallest_complex_pencil_eigenvalues(k_sp.as_ref(), m_sp.as_ref(), n_request)
        .expect("sparse complex eigensolve")
    };
    eprintln!(
        "eigensolve wall-clock: {:.3} s",
        t_solve.elapsed().as_secs_f64()
    );

    // Spurious filter (issue #696): anything with |λ| below
    // `NULL_TOL_REL · σ` is treated as gradient-kernel noise. The cutoff is
    // relative to the shift, not to the largest returned |λ| (the old
    // `1e-3 × max|λ|` rule let a single stray large Ritz value push the
    // cutoff past TM_1,1). The null cluster sits at |λ| ~ 1e-14 and the
    // lowest physical k² ≈ 1.5, so σ/2 = 0.5 has a wide margin both ways.
    // Applied to the dense-oracle path too (same pencil, same null cluster).
    let spurious_threshold = NULL_TOL_REL * LANCZOS_SIGMA;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one mode above spurious threshold");

    eprintln!(
        "spurious threshold |λ| = {:.3e}, first physical mode at index {}",
        spurious_threshold, first_physical
    );

    // Convert λ = k² to k on the branch Re(k) ≥ 0.
    lambdas
        .iter()
        .skip(first_physical)
        .take(n_modes)
        .map(|lam| {
            // Cancellation-free principal √λ (Re k ≥ 0, sign Im k = sign Im λ; #830).
            geode_core::eigen::wavenumber::principal_sqrt(*lam)
        })
        .collect()
}

/// Multiplicity-aware mode pairing (issues #40, #43, #1000).
///
/// Sorts the FEM modes by `Re(k)` and pairs them with the lowest analytic
/// roots whose multiplicities `2 l + 1` sum to the FEM count (the
/// `N_ANALYTIC_GROUPS` prefix in a normal run). Each FEM mode gets the
/// `(pol, l, n)` label of its group and a slot index `m_idx ∈ [0, 2l]`.
///
/// **Cluster pairing (issue #1000).** The FEM spectrum is split into
/// contiguous clusters by multiplet, and each cluster is then matched to a
/// catalog root of the same size. The analytic `k` order does not decide
/// the split. Every ordering of the groups' multiplicities defines one
/// contiguous partition of the sorted modes. The partition with the
/// smallest within-cluster scatter in the complex plane, measured on
/// `(Re k, |Im k|)`, is kept, because the members of a degenerate
/// multiplet share one complex `k`. When two roots have the same
/// multiplicity, the tie is broken by the smallest summed relative error
/// `|mean Re k − k_analytic| / k_analytic`. This step exists because
/// after #986 the catalog puts TM_2,1 (1.81333) below TE_1,1 (1.86880),
/// while the UPML pencil on the bundled fixture puts a tight TE_1,1-like
/// triplet (Q ≈ 9) below the TM_2,1 quintet (Q ≈ 31 – 48). Claiming FEM
/// modes in analytic `k` order gave modes 3 – 7 the TM_2,1 label, which
/// mixed the triplet with two quintet members. A pure largest-gap split
/// fails on that fixture too: the quintet splits 3 + 2 under the mesh's
/// near-cubic symmetry, and its internal `Re k` gap (1.6 %) is wider than
/// the triplet-to-quintet gap (1.5 %). The `|Im k|` axis separates them.
///
/// **Ambiguity gate (issue #43).** A cluster's rows are flagged
/// `ambiguous` when its root has another paired root within
/// `CLOSE_PAIR_GAP_FRAC`, and either of two things holds:
///
/// - the segmentation is not decisive, meaning the best partition with a
///   different multiplicity sequence scatters less than
///   `SEGMENTATION_MARGIN` times the chosen one; or
/// - the close neighbour has the same multiplicity, so only `k` order
///   tells the two labels apart.
///
/// **Fallback.** When the FEM count is not a prefix sum of catalog
/// multiplicities (for example, the solver returned fewer modes), or the
/// prefix has more than `MAX_CLUSTER_GROUPS` groups,
/// [`pair_modes_k_order`] runs the legacy ascending-`k` claim instead. That
/// path flags unfilled multiplets as `incomplete`.
///
/// The `Im(k)` banding note is a soft sanity check. Within each claimed
/// group it logs a warning when the per-slot `|Im k|` spread exceeds ~10 %.
/// The coarse fixture often breaks this band through mesh asymmetry, so
/// the note is never enforced.
fn pair_modes(analytic: &[MieRoot], fem: &[faer::c64]) -> Vec<Row> {
    let mut fem_sorted: Vec<faer::c64> = fem.to_vec();
    fem_sorted.sort_by(|a, b| a.re.total_cmp(&b.re));

    let mut covered = 0_usize;
    let mut n_groups = None;
    for (i, root) in analytic.iter().enumerate() {
        covered += root.multiplicity;
        if covered == fem_sorted.len() {
            n_groups = Some(i + 1);
            break;
        }
        if covered > fem_sorted.len() {
            break;
        }
    }

    match n_groups {
        Some(g) if g <= MAX_CLUSTER_GROUPS => pair_modes_by_cluster(&analytic[..g], &fem_sorted),
        _ => {
            eprintln!(
                "  note: {} FEM modes do not fill a catalog prefix of at most {} \
                 groups; falling back to the ascending-k claim",
                fem_sorted.len(),
                MAX_CLUSTER_GROUPS
            );
            pair_modes_k_order(analytic, &fem_sorted)
        }
    }
}

/// Largest number of analytic groups [`pair_modes`] will cluster-pair.
/// It enumerates all `G!` group orderings, so 7 (5040 orderings) bounds the
/// cost. A normal run pairs `N_ANALYTIC_GROUPS = 3`.
const MAX_CLUSTER_GROUPS: usize = 7;

/// The FEM segmentation counts as decisive when the best partition with
/// a different multiplicity sequence scatters at least this many times
/// more than the chosen one (issue #1000). On the bundled fixture the
/// ratio is about 7.4.
const SEGMENTATION_MARGIN: f64 = 2.0;

/// Sum of squared distances from the centroid in the `(Re k, |Im k|)`
/// plane. Degenerate multiplet members share one complex `k`, so a
/// correctly segmented multiplet has a small scatter.
fn cluster_scatter(modes: &[faer::c64]) -> f64 {
    let n = modes.len() as f64;
    let mean_re = modes.iter().map(|k| k.re).sum::<f64>() / n;
    let mean_im = modes.iter().map(|k| k.im.abs()).sum::<f64>() / n;
    modes
        .iter()
        .map(|k| (k.re - mean_re).powi(2) + (k.im.abs() - mean_im).powi(2))
        .sum()
}

/// Every ordering of `0..n` (`n!` entries, lexicographic order).
fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn rec(prefix: &mut Vec<usize>, used: &mut [bool], out: &mut Vec<Vec<usize>>) {
        if prefix.len() == used.len() {
            out.push(prefix.clone());
            return;
        }
        for i in 0..used.len() {
            if !used[i] {
                used[i] = true;
                prefix.push(i);
                rec(prefix, used, out);
                prefix.pop();
                used[i] = false;
            }
        }
    }
    let mut out = Vec::new();
    rec(&mut Vec::with_capacity(n), &mut vec![false; n], &mut out);
    out
}

/// Log the soft `Im(k)` banding note for one claimed multiplet.
fn im_band_note(root: &MieRoot, group: &[faer::c64]) {
    if group.len() < 2 {
        return;
    }
    let im_min = group
        .iter()
        .map(|k| k.im.abs())
        .fold(f64::INFINITY, f64::min);
    let im_max = group.iter().map(|k| k.im.abs()).fold(0.0_f64, f64::max);
    let band = if im_min > 0.0 {
        (im_max - im_min) / im_min
    } else {
        f64::INFINITY
    };
    if band > 0.10 {
        eprintln!(
            "  note: {}_{},{} multiplet Im(k) band = {:.1}% > 10% (mesh asymmetry)",
            root.pol.as_str(),
            root.l,
            root.n,
            band * 100.0
        );
    }
}

/// Build the rows for one claimed multiplet.
fn group_rows(root: &MieRoot, group: &[faer::c64], incomplete: bool, ambiguous: bool) -> Vec<Row> {
    group
        .iter()
        .enumerate()
        .map(|(slot, fem_k)| Row {
            pol: root.pol.as_str(),
            l: root.l,
            n: root.n,
            m_idx: slot,
            incomplete,
            ambiguous,
            analytic_k: root.k,
            fem_re_k: fem_k.re,
            fem_im_k: fem_k.im,
            rel_err_re_k: (fem_k.re - root.k).abs() / root.k,
            q: eigen::q_factor(*fem_k),
        })
        .collect()
}

/// Cluster pairing for [`pair_modes`] (issue #1000). `groups` is the
/// catalog prefix whose multiplicities sum to `fem_sorted.len()`, and
/// `fem_sorted` is sorted by `Re(k)`.
fn pair_modes_by_cluster(groups: &[MieRoot], fem_sorted: &[faer::c64]) -> Vec<Row> {
    // For each ordering: the contiguous partition's total scatter, then the
    // summed relative error of each cluster's mean Re k against its root.
    let score = |order: &[usize]| -> (f64, f64) {
        let mut cursor = 0;
        let mut scatter = 0.0;
        let mut k_cost = 0.0;
        for &g in order {
            let root = &groups[g];
            let cluster = &fem_sorted[cursor..cursor + root.multiplicity];
            scatter += cluster_scatter(cluster);
            let mean_re = cluster.iter().map(|k| k.re).sum::<f64>() / cluster.len() as f64;
            k_cost += (mean_re - root.k).abs() / root.k;
            cursor += root.multiplicity;
        }
        (scatter, k_cost)
    };
    let mults =
        |order: &[usize]| -> Vec<usize> { order.iter().map(|&g| groups[g].multiplicity).collect() };

    let orders = permutations(groups.len());
    let scored: Vec<(Vec<usize>, f64, f64)> = orders
        .into_iter()
        .map(|o| {
            let (s, k) = score(&o);
            (o, s, k)
        })
        .collect();
    // Orderings with the same multiplicity sequence give the same partition
    // and therefore bit-identical scatter, so this comparison is exact on
    // the scatter and only falls through to `k_cost` within one partition.
    let (best, best_scatter, _) = scored
        .iter()
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)))
        .expect("at least one group");
    let best_mults = mults(best);
    let runner_up = scored
        .iter()
        .filter(|(o, _, _)| mults(o) != best_mults)
        .map(|(_, s, _)| *s)
        .fold(f64::INFINITY, f64::min);
    // Only one distinct partition exists, or the runner-up is clearly worse.
    let decisive = runner_up > SEGMENTATION_MARGIN * best_scatter;
    if !decisive {
        eprintln!(
            "  warn: FEM multiplet segmentation is not decisive: best scatter {:.3e}, \
             runner-up {:.3e} (< {}x)",
            best_scatter, runner_up, SEGMENTATION_MARGIN
        );
    }

    let mut rows = Vec::new();
    let mut cursor = 0;
    for &g in best {
        let root = &groups[g];
        let group = &fem_sorted[cursor..cursor + root.multiplicity];
        cursor += root.multiplicity;
        im_band_note(root, group);

        let close = groups.iter().enumerate().filter(|&(h, other)| {
            h != g && (other.k - root.k).abs() / root.k < CLOSE_PAIR_GAP_FRAC
        });
        let mut ambiguous = false;
        for (_, other) in close {
            let same_mult = other.multiplicity == root.multiplicity;
            if !decisive || same_mult {
                ambiguous = true;
                eprintln!(
                    "  warn: close-pair gating: {}_{},{} (k = {:.5}) is within {:.3}% of \
                     {}_{},{} (k = {:.5}) and {}; claim is ambiguous",
                    root.pol.as_str(),
                    root.l,
                    root.n,
                    root.k,
                    (other.k - root.k).abs() / root.k * 100.0,
                    other.pol.as_str(),
                    other.l,
                    other.n,
                    other.k,
                    if same_mult {
                        "has the same multiplicity, so only k order separates them"
                    } else {
                        "the FEM segmentation is not decisive"
                    },
                );
            }
        }

        rows.extend(group_rows(root, group, false, ambiguous));
    }
    rows
}

/// Legacy ascending-`k` multiplicity claim (issues #40, #43). It is the
/// fallback for [`pair_modes`] when the FEM count does not fill a catalog
/// prefix.
///
/// Walks the catalog in ascending-`k` order and claims the next
/// `2 l + 1` FEM modes for each root. A multiplet that runs out of FEM
/// modes is emitted with `incomplete = true`. A group is flagged
/// `ambiguous` when the next root is within `CLOSE_PAIR_GAP_FRAC` and the
/// group's FEM `Re k` spread exceeds half the inter-root gap.
fn pair_modes_k_order(analytic: &[MieRoot], fem_sorted: &[faer::c64]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut cursor = 0_usize;

    for (g_idx, root) in analytic.iter().enumerate() {
        if cursor >= fem_sorted.len() {
            break;
        }
        let mult = root.multiplicity;
        let take = mult.min(fem_sorted.len() - cursor);
        let group = &fem_sorted[cursor..cursor + take];
        let incomplete = take < mult;
        im_band_note(root, group);

        let mut ambiguous = false;
        if let Some(next) = analytic.get(g_idx + 1) {
            let gap = (next.k - root.k).abs();
            let rel_gap = gap / root.k;
            if rel_gap < CLOSE_PAIR_GAP_FRAC && group.len() >= 2 {
                let re_min = group.iter().map(|k| k.re).fold(f64::INFINITY, f64::min);
                let re_max = group.iter().map(|k| k.re).fold(f64::NEG_INFINITY, f64::max);
                let spread = re_max - re_min;
                if spread > 0.5 * gap {
                    ambiguous = true;
                    eprintln!(
                        "  warn: close-pair gating: {}_{},{} (k = {:.5}) next root \
                         {}_{},{} (k = {:.5}, rel gap = {:.3}%) and within-multiplet \
                         FEM spread = {:.3e} > 0.5 * gap ({:.3e}); claim is ambiguous",
                        root.pol.as_str(),
                        root.l,
                        root.n,
                        root.k,
                        next.pol.as_str(),
                        next.l,
                        next.n,
                        next.k,
                        rel_gap * 100.0,
                        spread,
                        0.5 * gap,
                    );
                }
            }
        }

        rows.extend(group_rows(root, group, incomplete, ambiguous));
        cursor += take;
    }

    rows
}

fn print_table(rows: &[Row]) {
    eprintln!();
    eprintln!(
        "{:>3}  {:>12}  {:>4}  {:>11}  {:>11}  {:>11}  {:>12}  {:>10}",
        "i", "mode", "m", "analytic k", "FEM Re(k)", "FEM Im(k)", "rel err Re(k)", "Q"
    );
    eprintln!(
        "{}",
        "-".repeat(3 + 12 + 4 + 11 + 11 + 11 + 12 + 10 + 7 * 2)
    );
    for (i, r) in rows.iter().enumerate() {
        let label = format!("{}_{},{}", r.pol, r.l, r.n);
        let suffix = match (r.incomplete, r.ambiguous) {
            (true, true) => "*?",
            (true, false) => "* ",
            (false, true) => "? ",
            (false, false) => "  ",
        };
        eprintln!(
            "{:>3}  {:>10}{}  {:>4}  {:>11.5}  {:>11.5}  {:>11.5e}  {:>12.3}%  {:>10.3e}",
            i,
            label,
            suffix,
            r.m_idx,
            r.analytic_k,
            r.fem_re_k,
            r.fem_im_k,
            r.rel_err_re_k * 100.0,
            r.q,
        );
    }
    if rows.iter().any(|r| r.incomplete) {
        eprintln!("  (* = analytic multiplicity not fully filled by FEM modes)");
    }
    if rows.iter().any(|r| r.ambiguous) {
        eprintln!("  (? = close-pair overlap; pairing label is best-guess, issue #43)");
    }
    eprintln!();
}

fn emit_results(
    rows: &[Row],
    path: &Path,
    scalar_pml: bool,
    n_modes: usize,
    backend: &BackendInfo,
) {
    let commit = geode_util::repo::current_commit();
    let pml_kind = if scalar_pml { "scalar" } else { "anisotropic" };

    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p mie_sphere --release`.\n");
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Quoted by the README Mie-eigenmode section; no test reads this file\n");
    s.push_str("# (`crates/geode-core/tests/mie_sphere.rs` re-solves live).\n");
    s.push('\n');

    s.push_str("[meta]\n");
    s.push_str("description = \"Mie sphere benchmark (issue #4 v1, issue #40, issue #54): FEM eigenmodes vs. extended analytic PEC-cavity catalog (l ∈ [1,4], TE+TM, n ∈ [1,5]) with multiplicity-claim mode classification.\"\n");
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    backend.push_meta(&mut s);
    s.push_str(&format!("pml_kernel = \"{pml_kind}\"\n"));
    s.push_str(&format!("n_inside = {}\n", N_INSIDE));
    s.push_str(&format!("sigma_0 = {}\n", SIGMA_0));
    if !scalar_pml {
        s.push_str(&format!("k0_ref = {}\n", K0_REF));
    }
    s.push_str(&format!("r_sphere = {}\n", R_SPHERE));
    s.push_str(&format!("r_buffer = {}\n", R_BUFFER));
    s.push_str(&format!("n_modes = {}\n", n_modes));
    s.push_str(&format!("l_max = {}\n", L_MAX));
    s.push_str(&format!("n_max_radial = {}\n", N_MAX));
    s.push_str("notes = [\n");
    s.push_str(
        "  \"Analytic side is PEC-cavity dielectric resonator, not open-space Mie WGM.\",\n",
    );
    s.push_str("  \"Real analytic roots only; complex open-space Mie roots are a separate axis (#33).\",\n");
    s.push_str("  \"Mode classification (issue #1000): split the Re(k)-sorted FEM modes into contiguous multiplets by least (Re k, |Im k|) scatter over orderings of the catalog multiplicities, then match each cluster to a root of size 2l+1 (ascending-k claim only as a fallback).\",\n");
    if scalar_pml {
        s.push_str(
            "  \"FEM side: scalar isotropic PML (legacy --scalar-pml path), bundled 774-node refined fixture (issue #49). Has ~16% h-independent reflection ceiling on TM_1,1.\",\n",
        );
    } else {
        s.push_str(
            "  \"FEM side: anisotropic UPML diagonal complex permittivity tensor (default, issue #54), bundled 774-node refined fixture (issue #49). Breaks the 16% scalar ceiling (that scalar figure predates the issue #986 root correction and was not re-measured) — TM_1,1 ≈ 3.6% rel err against the corrected root.\",\n",
        );
        s.push_str(
            "  \"For s_r = s_t = 1 - jσ/ω the off-diagonal rotation terms are identically zero, so the diagonal-only tensor is mathematically exact (not an approximation) for this profile.\",\n",
        );
    }
    s.push_str("  \"Driven scattering benchmark (Q_ext vs. ka) is v2 (separate scope).\",\n");
    s.push_str("]\n");
    s.push('\n');

    geode_util::fixture::push_rows(&mut s, "mode", rows);

    geode_util::fixture::write_toml(path, &s).expect("write mie_sphere results.toml");
}

/// Mid-`ka` operating point of the driven benchmark sweep
/// (`mie_driven_scattering`'s `KA_VALUES = [1.0, 1.5, 1.9, 2.4, 3.0]`),
/// used as the default frequency for `--export-field`. `ω = ka / R_SPHERE`.
const EXPORT_KA: f64 = 1.9;

/// Matched-UPML strength for the `--export-field` driven solve — same
/// value as `mie_driven_scattering` (continuum round-trip attenuation
/// `exp(−2σ₀d/3) ≈ 2e-4`).
const EXPORT_SIGMA_0: f64 = 25.0;

/// Opt-in `--export-field` path (Epic #276 Phase 2B, issue #287):
/// solve the bundled sphere once as a driven scattering problem at the
/// mid-`ka` point and dump the scattered near field to `<out_dir>/E_mie.vtu`.
///
/// `out_dir` is the resolved (already-created) artifact directory from
/// [`geode_app::OutputDir`]. Independent of the eigenmode benchmark —
/// does not write `results.toml`.
fn export_field<B: Backend>(_device: &B::Device, out_dir: &Path) {
    let f = read_sphere_fixture().expect("sphere fixture load for --export-field");
    let omega = EXPORT_KA / R_SPHERE;
    eprintln!(
        "=== --export-field: driven scattered-field solve at ka = {EXPORT_KA} \
         (omega = {omega:.5}), matched UPML sigma_0 = {EXPORT_SIGMA_0} ==="
    );

    let (_mask_edges, interior) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let j_at = plane_wave_polarization_current(
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        omega,
    );
    let sol = solve_scattered_field_matched_upml(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        &interior,
        N_INSIDE,
        EXPORT_SIGMA_0,
        omega,
        j_at,
    )
    .expect("matched-UPML scattered-field solve for --export-field");
    eprintln!(
        "  solve residual_rel = {:.3e}; reconstructing per-node E (Whitney average)",
        sol.residual_rel
    );

    let (e_re, e_im) = edge_field_to_nodes(&f.mesh, &sol.e_edges);

    // Per-node eps_r: n² inside the sphere, 1 outside. Average the
    // per-tet relative permittivity over the tets incident to each node.
    let eps_inside = N_INSIDE * N_INSIDE;
    let mut eps_sum = vec![0.0_f64; f.mesh.n_nodes()];
    let mut eps_cnt = vec![0_u32; f.mesh.n_nodes()];
    for (t, tet) in f.mesh.tets.iter().enumerate() {
        let eps = if f.tet_physical_tags[t] == PHYS_SPHERE_INTERIOR {
            eps_inside
        } else {
            1.0
        };
        for &v in tet.iter() {
            eps_sum[v as usize] += eps;
            eps_cnt[v as usize] += 1;
        }
    }
    let eps_r: Vec<f64> = eps_sum
        .iter()
        .zip(eps_cnt.iter())
        .map(|(&s, &c)| if c > 0 { s / c as f64 } else { 1.0 })
        .collect();

    // `out_dir` is already created by `OutputDir::resolve`; the exported
    // file lives at `<out_dir>/E_mie.vtu` (fixed filename).
    let out = out_dir.join("E_mie.vtu");
    geode_core::postproc::viz::write_vtu(&out, &f.mesh, &e_re, Some(&e_im), Some(&eps_r))
        .expect("write --export-field .vtu");
    eprintln!(
        "  wrote {} ({} nodes, {} tets)",
        out.display(),
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );
}

/// Mie-sphere benchmark CLI.
///
/// Flattens the shared `geode-app` `--out-dir` / `-v`/`-q` groups and
/// keeps the three example-local toggles (`--dense`, `--scalar-pml`,
/// `--export-field`) the original hand-rolled argv recognised.
#[derive(Parser)]
#[command(about = "Mie sphere benchmark: FEM eigenmodes vs. analytic PEC-cavity roots (issue #4).")]
struct Args {
    /// Use the dense `FaerComplexEigensolver` correctness oracle instead
    /// of the default sparse shift-invert Lanczos path.
    #[arg(long)]
    dense: bool,

    /// Use the legacy scalar-isotropic complex-ε PML (~16% TM_1,1 rel
    /// err ceiling) instead of the default anisotropic UPML.
    #[arg(long = "scalar-pml")]
    scalar_pml: bool,

    /// Export the driven scattered near field to `<out-dir>/E_mie.vtu`
    /// (mid-`ka` point) instead of running the eigenmode benchmark.
    #[arg(long = "export-field")]
    export_field: bool,

    #[command(flatten)]
    out: OutputDir,

    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        type B = TestBackend;
        let device = <B as BackendTypes>::Device::default();

        // Opt-in field export (issue #287). Short-circuits the eigenmode
        // benchmark so a normal run produces an unchanged `results.toml`.
        if self.export_field {
            let dir = self.out.resolve()?;
            export_field::<B>(&device, &dir);
            return Ok(());
        }

        let use_dense = self.dense;
        let scalar_pml = self.scalar_pml;

        eprintln!("=== Mie sphere benchmark (issue #4 v1, issue #40, issue #54) ===");
        if use_dense {
            eprintln!("  eigensolver: DENSE (correctness oracle, --dense flag)");
        } else {
            eprintln!("  eigensolver: SPARSE Lanczos (default, pass --dense to switch)");
        }
        if scalar_pml {
            eprintln!("  PML kernel: SCALAR isotropic (--scalar-pml, legacy 16% ceiling)");
        } else {
            eprintln!(
                "  PML kernel: ANISOTROPIC UPML diagonal (default, issue #54; \
                 pass --scalar-pml for the legacy cross-check)"
            );
        }
        eprintln!();
        eprintln!(
            "Fixture geometry: R_sphere = {R_SPHERE}, R_buffer = {R_BUFFER}, n_inside = {N_INSIDE}",
        );
        eprintln!("PML absorption: σ₀ = {SIGMA_0}");
        if !scalar_pml {
            eprintln!("PML reference wavenumber: k₀_ref = {K0_REF}");
        }
        eprintln!();

        // Analytic ground truth: extended catalog with l ∈ [1, L_MAX],
        // both TE and TM polarisations, lowest N_MAX radial overtones each.
        // Each MieRoot carries its (l, n, pol, multiplicity = 2l+1) label.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let n_modes = n_modes_from_catalog(&analytic);
        eprintln!(
            "Analytic catalog: {} roots over l ∈ [1, {}], TE+TM, n ∈ [1, {}]",
            analytic.len(),
            L_MAX,
            N_MAX
        );
        eprintln!(
            "Catalog-derived N_MODES = {} (sum of multiplicities of first {} groups, issue #43)",
            n_modes, N_ANALYTIC_GROUPS,
        );
        eprintln!("Lowest 12 analytic roots (PEC-cavity dielectric resonator):");
        for r in analytic.iter().take(12) {
            eprintln!(
                "  {}_{},{}  k = {:.5}  k² = {:.5}  mult = {}",
                r.pol.as_str(),
                r.l,
                r.n,
                r.k,
                r.k * r.k,
                r.multiplicity,
            );
        }

        // FEM eigensolve.
        eprintln!();
        eprintln!("=== FEM eigensolve ===");
        let fem_k = fem_complex_k::<B>(&device, use_dense, scalar_pml, n_modes);
        eprintln!("Lowest {} physical FEM modes (k = sqrt(λ)):", fem_k.len());
        for (i, k) in fem_k.iter().enumerate() {
            eprintln!("  mode[{i}]  k = {:.5} + {:.5e}i", k.re, k.im);
        }

        // Pair and report.
        let rows = pair_modes(&analytic, &fem_k);
        print_table(&rows);

        // Persist.
        emit_results(
            &rows,
            &results_path(),
            scalar_pml,
            n_modes,
            &BackendInfo::of::<B>(&device),
        );

        // Issue #33 — open-space Mie WGM cross-check.
        //
        // The PEC-cavity table above is the σ₀ → 0 closed-shell limit. The
        // open-space catalog `OPEN_SPACE_WGM_TABLE_N15` is the genuinely
        // radiative target: complex `k`, outgoing Hankel waves, no PEC
        // outer wall. We print a side-by-side for the lowest few FEM modes
        // so the reviewer can see the magnitude of the residual gap that
        // tighter PML profiles (issue #35) and finer meshes need to close.
        let open_space = open_space_wgm_roots_n15();
        eprintln!();
        eprintln!("=== Open-space Mie WGM cross-check (issue #33) ===");
        eprintln!("Lowest 8 open-space WGM roots (n = 1.5, R_s = 1.0; sign convention Im(k) < 0):");
        for r in open_space.iter().take(8) {
            eprintln!(
                "  {}_{},{}  k = {:.5} + {:.5e}i  Q = {:.3}",
                r.pol.as_str(),
                r.l,
                r.n,
                r.re_k,
                r.im_k,
                r.q()
            );
        }
        eprintln!();
        eprintln!("Closest open-space WGM for each FEM mode (by |Δk|):");
        eprintln!(
            "{:>3}  {:>12}  {:>11}  {:>11}  {:>12}  {:>12}",
            "i", "mode", "FEM Re(k)", "WGM Re(k)", "rel err Re(k)", "Q ratio"
        );
        eprintln!("{}", "-".repeat(70));
        for (i, fk) in fem_k.iter().enumerate() {
            // Closest in (Re(k), |Im(k)|) Euclidean metric.
            let best = open_space
                .iter()
                .min_by(|a, b| {
                    let da = (a.re_k - fk.re).hypot(a.im_k.abs() - fk.im.abs());
                    let db = (b.re_k - fk.re).hypot(b.im_k.abs() - fk.im.abs());
                    da.partial_cmp(&db).unwrap()
                })
                .expect("non-empty open-space catalog");
            let fem_q = if fk.im.abs() > 1e-12 {
                fk.re / (2.0 * fk.im.abs())
            } else {
                f64::INFINITY
            };
            let rel_err = (fk.re - best.re_k).abs() / best.re_k;
            let q_ratio = fem_q / best.q();
            eprintln!(
                "{:>3}  {:>9}_{},{}  {:>11.5}  {:>11.5}  {:>11.3}%  {:>12.3}",
                i,
                best.pol.as_str(),
                best.l,
                best.n,
                fk.re,
                best.re_k,
                rel_err * 100.0,
                q_ratio
            );
        }
        eprintln!();
        eprintln!("Note: 30–40 % rel err Re(k) and large Q ratios are expected on the");
        eprintln!("bundled fixture — the PML-truncated FEM sits between PEC cavity and");
        eprintln!("true open space. Tightening the gap is the target of #35.");
        eprintln!();

        eprintln!("=== Done ===");
        Ok(())
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<Args>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use geode_core::analytic::mie::MiePolarisation;

    /// Synthetic multiplet of `mult` FEM modes spread uniformly over
    /// `spread` in Re(k) around `k`, with uniform damping `im`.
    fn synth_multiplet(k: f64, mult: usize, spread: f64, im: f64) -> Vec<faer::c64> {
        (0..mult)
            .map(|i| {
                let t = if mult > 1 {
                    i as f64 / (mult - 1) as f64 - 0.5
                } else {
                    0.0
                };
                faer::c64::new(k + t * spread, im)
            })
            .collect()
    }

    /// Locate the canonical close pair on the bundled n = 1.5 catalog:
    /// TM_2,1 (k ≈ 1.81333) immediately followed by TE_1,1
    /// (k ≈ 1.86880), 3.06 % apart (issue #986 corrected these roots;
    /// before it the catalog had TE_1,1 / TM_2,1 at 1.88943 / 1.89074).
    fn tm21_te11_pair(analytic: &[MieRoot]) -> (usize, MieRoot, MieRoot) {
        let i_tm21 = analytic
            .iter()
            .position(|r| r.pol == MiePolarisation::TM && r.l == 2 && r.n == 1)
            .expect("TM_2,1 in catalog");
        let tm21 = analytic[i_tm21];
        let te11 = analytic
            .get(i_tm21 + 1)
            .copied()
            .expect("TE_1,1 follows TM_2,1");
        assert_eq!(te11.pol, MiePolarisation::TE);
        assert_eq!(te11.l, 1);
        assert_eq!(te11.n, 1);
        (i_tm21, tm21, te11)
    }

    /// `(pol, l, n)` labels of `rows`, in row order.
    fn labels(rows: &[Row]) -> Vec<(&'static str, usize, usize)> {
        rows.iter().map(|r| (r.pol, r.l, r.n)).collect()
    }

    /// The 11 lowest physical modes of the bundled 774-node fixture
    /// (anisotropic UPML, sparse Lanczos), as `(Re k, Im k)` and recorded
    /// in `benchmarks/mie_sphere/results.toml` before issue #1000. The
    /// fixture holds a TM_1,1 triplet (Q ≈ 27), a TE_1,1-like triplet at
    /// 1.870 – 1.872 (Q ≈ 9), and a quintet at 1.900 – 1.934 (Q ≈ 31 –
    /// 48) that splits 3 + 2 in Re k.
    const BUNDLED_FEM: [(f64, f64); 11] = [
        (1.2292958593564434, 0.022509717187390272),
        (1.2296258642908768, 0.022531291740495837),
        (1.2298099305982382, 0.022780785602638597),
        (1.869996159595324, 0.10430146398782289),
        (1.871585864076684, 0.1040480325066081),
        (1.8723054768064273, 0.10495466984526487),
        (1.8997827635357853, 0.019608146372213784),
        (1.9013881741024847, 0.019679550777177722),
        (1.902372705279823, 0.020431853945243782),
        (1.9329648533985508, -0.030588754682209115),
        (1.9340619452982504, -0.03117942620765935),
    ];

    #[test]
    fn bundled_spectrum_pairs_te11_triplet_below_tm21_quintet() {
        // Issue #1000 regression: the analytic k order (TM_1,1, TM_2,1,
        // TE_1,1) disagrees with the UPML pencil's multiplet order
        // (triplet, triplet, quintet). The cluster pairing must give the
        // Q ≈ 9 triplet the TE_1,1 label and the quintet the TM_2,1 label,
        // without flagging either group as ambiguous.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let fem: Vec<faer::c64> = BUNDLED_FEM
            .iter()
            .map(|&(re, im)| faer::c64::new(re, im))
            .collect();
        let rows = pair_modes(&analytic, &fem);
        assert_eq!(rows.len(), 11);

        let tm11 = ("TM", 1, 1);
        let te11 = ("TE", 1, 1);
        let tm21 = ("TM", 2, 1);
        let expected = [
            tm11, tm11, tm11, te11, te11, te11, tm21, tm21, tm21, tm21, tm21,
        ];
        assert_eq!(labels(&rows), expected);
        assert!(rows.iter().all(|r| !r.ambiguous && !r.incomplete));
        // Slots run 0..2l within each multiplet.
        let slots: Vec<usize> = rows.iter().map(|r| r.m_idx).collect();
        assert_eq!(slots, [0, 1, 2, 0, 1, 2, 0, 1, 2, 3, 4]);
        // TE_1,1 rows sit within 0.2 % of the corrected root.
        assert!(
            rows[3..6].iter().all(|r| r.rel_err_re_k < 2e-3),
            "TE_1,1 triplet rel err: {:?}",
            rows[3..6]
                .iter()
                .map(|r| r.rel_err_re_k)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cluster_pairing_follows_catalog_order_when_spectrum_does() {
        // FEM multiplets in the same order as the catalog (TM_2,1 quintet
        // below the TE_1,1 triplet), each tight. The labels must follow the
        // catalog, and no row may be ambiguous.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let (i_tm21, tm21, te11) = tm21_te11_pair(&analytic);
        assert_eq!(i_tm21, 1, "TM_2,1 is the second catalog root");
        let gap = te11.k - tm21.k;
        assert!(
            gap / tm21.k < CLOSE_PAIR_GAP_FRAC,
            "TM_2,1 / TE_1,1 must be a close pair (got {:.5}%)",
            gap / tm21.k * 100.0
        );

        let ground = analytic[0];
        let mut fem: Vec<faer::c64> = Vec::new();
        fem.extend(synth_multiplet(ground.k, ground.multiplicity, 1e-3, 1e-2));
        fem.extend(synth_multiplet(tm21.k, tm21.multiplicity, 0.1 * gap, 2e-2));
        fem.extend(synth_multiplet(te11.k, te11.multiplicity, 1e-4, 1e-1));

        let rows = pair_modes(&analytic, &fem);
        let tm11 = ("TM", 1, 1);
        let te11_l = ("TE", 1, 1);
        let tm21_l = ("TM", 2, 1);
        let expected = [
            tm11, tm11, tm11, tm21_l, tm21_l, tm21_l, tm21_l, tm21_l, te11_l, te11_l, te11_l,
        ];
        assert_eq!(labels(&rows), expected);
        assert!(rows.iter().all(|r| !r.ambiguous));
    }

    #[test]
    fn close_pair_flags_ambiguous_when_segmentation_is_indecisive() {
        // The TM_2,1 quintet and TE_1,1 triplet smear into one evenly
        // spaced run of 8 modes with equal damping, so a (5, 3) split and
        // a (3, 5) split scatter equally. The close-pair rows must be
        // flagged ambiguous; the well-separated ground triplet must not.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let (_, tm21, te11) = tm21_te11_pair(&analytic);
        let ground = analytic[0];
        let mut fem: Vec<faer::c64> = Vec::new();
        fem.extend(synth_multiplet(ground.k, ground.multiplicity, 1e-3, 1e-2));
        fem.extend(synth_multiplet(
            0.5 * (tm21.k + te11.k),
            8,
            te11.k - tm21.k,
            2e-2,
        ));

        let rows = pair_modes(&analytic, &fem);
        assert_eq!(rows.len(), 11);
        assert!(rows[..3].iter().all(|r| !r.ambiguous && r.l == 1));
        assert!(
            rows[3..].iter().all(|r| r.ambiguous),
            "the TM_2,1 / TE_1,1 rows must be ambiguous: {:?}",
            labels(&rows)
        );
    }

    /// Hand-built catalog root for the pairing tests below.
    fn root(pol: MiePolarisation, l: usize, n: usize, k: f64) -> MieRoot {
        MieRoot {
            pol,
            l,
            n,
            k,
            multiplicity: 2 * l + 1,
        }
    }

    #[test]
    fn same_multiplicity_close_pair_is_ambiguous_and_tie_breaks_on_mean_re_k() {
        // Two triplet roots 5 % apart, plus a well-separated quintet. The
        // bundled catalog never reaches this branch, because its two
        // triplets (TM_1,1 and TE_1,1) are 57 % apart. Both triplet
        // orderings give the same (3, 3, 5) partition and so the same
        // scatter. Only the mean-Re-k tie-break tells the labels apart, so
        // both triplets must be flagged ambiguous even though the
        // segmentation itself is decisive.
        let catalog = [
            root(MiePolarisation::TM, 1, 1, 1.00),
            root(MiePolarisation::TE, 1, 1, 1.05),
            root(MiePolarisation::TM, 2, 1, 2.00),
        ];
        assert!((catalog[1].k - catalog[0].k) / catalog[0].k < CLOSE_PAIR_GAP_FRAC);

        // Each FEM triplet sits 1 % above its root, so the lower cluster
        // is nearer TM_1,1 and the upper one nearer TE_1,1.
        let mut fem: Vec<faer::c64> = Vec::new();
        fem.extend(synth_multiplet(1.01, 3, 1e-3, 1e-2));
        fem.extend(synth_multiplet(1.06, 3, 1e-3, 5e-2));
        fem.extend(synth_multiplet(2.02, 5, 1e-3, 2e-2));

        let rows = pair_modes(&catalog, &fem);
        let tm11 = ("TM", 1, 1);
        let te11 = ("TE", 1, 1);
        let tm21 = ("TM", 2, 1);
        let expected = [
            tm11, tm11, tm11, te11, te11, te11, tm21, tm21, tm21, tm21, tm21,
        ];
        assert_eq!(labels(&rows), expected);
        assert!(rows.iter().all(|r| !r.incomplete));
        assert!(
            rows[..6].iter().all(|r| r.ambiguous),
            "both same-multiplicity triplets must be ambiguous"
        );
        assert!(
            rows[6..].iter().all(|r| !r.ambiguous),
            "the well-separated quintet must not be ambiguous"
        );
    }

    /// Eight modes for a close quintet / triplet pair: three at `k0`, two
    /// at `k0 + d * span`, three at `k0 + span`, all with the same damping.
    /// Returns the modes and the runner-up scatter ratio between the
    /// (3, 5) and (5, 3) partitions, which is `((1 - d) / d)^2` for
    /// `d < 0.5`.
    fn straddling_pair_spectrum(d: f64) -> (Vec<faer::c64>, f64) {
        let (k0, span, im) = (1.80, 0.06, 2e-2);
        let fem: Vec<faer::c64> = [0.0, 0.0, 0.0, d, d, 1.0, 1.0, 1.0]
            .iter()
            .map(|&x| faer::c64::new(k0 + x * span, im))
            .collect();
        let s53 = cluster_scatter(&fem[..5]) + cluster_scatter(&fem[5..]);
        let s35 = cluster_scatter(&fem[..3]) + cluster_scatter(&fem[3..]);
        (fem, s35 / s53)
    }

    #[test]
    fn segmentation_margin_threshold_separates_ratio_1p5_from_2p25() {
        // A quintet and a triplet 3.3 % apart, with two modes straddling
        // the gap. Moving the straddlers sets the runner-up ratio, so the
        // two cases below sit on either side of `SEGMENTATION_MARGIN`.
        let catalog = [
            root(MiePolarisation::TM, 2, 1, 1.80),
            root(MiePolarisation::TE, 1, 1, 1.86),
        ];
        assert!((catalog[1].k - catalog[0].k) / catalog[0].k < CLOSE_PAIR_GAP_FRAC);
        let tm21 = ("TM", 2, 1);
        let te11 = ("TE", 1, 1);
        let expected = [tm21, tm21, tm21, tm21, tm21, te11, te11, te11];

        // Ratio about 1.49: the (5, 3) split still wins, but by less than
        // the margin, so every row is ambiguous.
        let (fem, ratio) = straddling_pair_spectrum(0.45);
        assert!(
            ratio > 1.0 && ratio < SEGMENTATION_MARGIN,
            "ratio {ratio} must sit between 1 and the margin"
        );
        let rows = pair_modes(&catalog, &fem);
        assert_eq!(labels(&rows), expected);
        assert!(rows.iter().all(|r| r.ambiguous));

        // Ratio 2.25: just past the margin, so no row is ambiguous.
        let (fem, ratio) = straddling_pair_spectrum(0.40);
        assert!(
            ratio > SEGMENTATION_MARGIN && ratio < 2.5,
            "ratio {ratio} must sit just above the margin"
        );
        let rows = pair_modes(&catalog, &fem);
        assert_eq!(labels(&rows), expected);
        assert!(rows.iter().all(|r| !r.ambiguous));
    }

    #[test]
    fn short_fem_spectrum_falls_back_to_k_order_claim() {
        // 10 FEM modes do not fill a catalog prefix (3, 8, 11, ...), so the
        // pairing falls back to the ascending-k claim. The last group is
        // then short of its multiplicity and must be flagged incomplete.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let (_, tm21, te11) = tm21_te11_pair(&analytic);
        let ground = analytic[0];
        let mut fem: Vec<faer::c64> = Vec::new();
        fem.extend(synth_multiplet(ground.k, ground.multiplicity, 1e-3, 1e-2));
        fem.extend(synth_multiplet(tm21.k, tm21.multiplicity, 1e-3, 2e-2));
        fem.extend(synth_multiplet(te11.k, 2, 1e-4, 1e-1));

        let rows = pair_modes(&analytic, &fem);
        assert_eq!(rows.len(), 10);
        let tail = &rows[8..];
        assert!(
            tail.iter()
                .all(|r| r.pol == "TE" && r.l == 1 && r.n == 1 && r.incomplete)
        );
        assert!(rows[..8].iter().all(|r| !r.incomplete));
    }

    #[test]
    fn k_order_fallback_flags_close_pair_overlap() {
        // Legacy gate on the fallback path (issue #43): the TM_2,1 claim
        // spreads over the whole gap to TE_1,1, so its rows are ambiguous.
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let (_, tm21, te11) = tm21_te11_pair(&analytic);
        let gap = te11.k - tm21.k;
        let ground = analytic[0];
        let mut fem: Vec<faer::c64> = Vec::new();
        fem.extend(synth_multiplet(ground.k, ground.multiplicity, 1e-3, 1e-2));
        fem.extend(synth_multiplet(tm21.k, tm21.multiplicity, gap, 1e-2));

        let rows = pair_modes_k_order(&analytic, &fem);
        let tm21_rows: Vec<&Row> = rows
            .iter()
            .filter(|r| r.pol == "TM" && r.l == 2 && r.n == 1)
            .collect();
        assert_eq!(tm21_rows.len(), 5);
        assert!(tm21_rows.iter().all(|r| r.ambiguous));
    }

    #[test]
    fn permutations_enumerates_all_orderings() {
        assert_eq!(permutations(1), vec![vec![0]]);
        let p3 = permutations(3);
        assert_eq!(p3.len(), 6);
        let mut sorted = p3.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 6);
    }

    #[test]
    fn n_modes_from_catalog_matches_sum_of_multiplicities() {
        // Catalog-derived N_MODES must equal the cumulative
        // multiplicity sum of the first N_ANALYTIC_GROUPS catalog
        // entries (issue #43, replaces hard-coded N_MODES = 8).
        let analytic = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
        let expected: usize = analytic
            .iter()
            .take(N_ANALYTIC_GROUPS)
            .map(|r| r.multiplicity)
            .sum();
        assert_eq!(n_modes_from_catalog(&analytic), expected);
        // For N_ANALYTIC_GROUPS = 3 on the bundled catalog, that's
        // TM_1,1 (mult 3) + TM_2,1 (mult 5) + TE_1,1 (mult 3) = 11 —
        // the catalog tracks automatically, no hand-tuned magic 8.
        assert!(
            n_modes_from_catalog(&analytic) >= 3,
            "must cover at least the TM_1,1 ground triplet"
        );
    }
}
