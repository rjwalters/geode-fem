//! Mie-sphere benchmark — FEM eigenmodes vs. analytic PEC-cavity
//! dielectric-sphere resonance roots (issue #4, north-star deliverable).
//!
//! **v1** (issue #40 hardening of the v0 in PR #39):
//!
//! - **Extended analytic catalog**: roots for `l ∈ [1, L_MAX]`, both TE
//!   and TM polarisations, lowest `N_MAX` radial overtones each — about
//!   40 entries in the [0.1, 20] `k` window. Computed via the same
//!   Newton+bisection scheme as v0, with Miller's downward recurrence
//!   for the spherical Bessel `j_l` at high `l` / small `x`.
//! - **Multiplicity-claim pairing**: walks the catalog in ascending-`k`
//!   order. For each analytic root, claims the next `2 l + 1` FEM
//!   modes (sorted by `Re(k)`) and labels each one with its slot
//!   `m_idx ∈ [0, 2l]` within the magnetic-degeneracy multiplet. v0
//!   used nearest-`k` pairing which mis-labeled the second FEM
//!   triplet as TM_1,1; v1 correctly identifies it as TE_1,1.
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
//!   tabulated in `geode_core::OPEN_SPACE_WGM_TABLE_N15` (issue #33)
//!   and printed as a side-by-side cross-check at the bottom of the
//!   run — they are the physically correct ground truth, but the
//!   PML-truncated FEM does not yet reach them tightly (~30–40 % rel
//!   err on `Re(k)` at the bundled fixture). Tightening that gap is
//!   the target of #35.
//! - **FEM side**: 774-node tet mesh (the bundled refined fixture
//!   from issue #49, bumped from the original 313 nodes), **anisotropic
//!   UPML** (diagonal complex permittivity tensor, issue #54) over the
//!   vacuum buffer, σ₀ = 5.0, k₀_ref = 2.0. Expect ~6 % relative
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
//! cargo run -p geode-core --release --example mie_sphere
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
//! cargo run -p geode-core --release --example mie_sphere -- --dense
//! cargo run -p geode-core --release --example mie_sphere -- --scalar-pml
//! ```
//!
//! `--release` is required because faer 0.24's `gevd` path panics
//! under `debug-assertions` (same root cause as `tests/sphere_pml_*`).
//! The sparse path is independent of `gevd` but the dense fallback
//! still needs release mode.
//!
//! Writes `benchmarks/mie_sphere/results.toml` relative to the
//! workspace root (located via `CARGO_MANIFEST_DIR`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use burn::tensor::backend::BackendTypes;
use faer::sparse::{SparseColMat, Triplet};

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec_with_anisotropic_epsilon,
    assemble_global_nedelec_with_complex_epsilon, build_anisotropic_pml_tensor_diag,
    build_complex_epsilon_r_pml, burn_complex_mass_to_faer, burn_matrix_to_faer, mie_roots_catalog,
    open_space_wgm_roots_n15, read_sphere_fixture, sphere_n_interior_nodes,
    sphere_pec_interior_edges, tet_centroid_radii, tet_centroids, upload_mesh, ComplexEigenSolver,
    DefaultBackend, FaerComplexEigensolver, MiePolarisation, MieRoot, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos, R_BUFFER, R_SPHERE,
};

type B = DefaultBackend;

/// Refractive index inside the sphere. n=1.5 is the textbook
/// B&H dielectric test case.
const N_INSIDE: f64 = 1.5;

/// PML absorption strength. σ₀ = 5.0 matches `tests/sphere_pml_*`.
const SIGMA_0: f64 = 5.0;

/// Reference wavenumber used to scale the anisotropic-UPML stretching
/// profiles `s_r = s_t = 1 - jσ/(ω₀ ε₀)` with ω₀ = k₀_ref. Matches the
/// `tests/sphere_pml_anisotropic_eigenmode.rs` acceptance test —
/// k₀_ref ≈ 2.0 is near the lowest physical mode's `Re(k)` and gives
/// the documented ~6% TM_1,1 rel err on the bundled fixture.
const K0_REF: f64 = 2.0;

/// Number of physical FEM modes to compare (above the gradient nullspace).
///
/// Bumped to 8 (v0 was 5) so the consecutive-multiplicity claim walks
/// at least two full analytic groups: the 3-fold TM_1,1 triplet
/// plus another (l, n, pol) cluster above it.
const N_MODES: usize = 8;

/// Maximum angular order in the analytic catalog (issue #40).
const L_MAX: usize = 4;
/// Maximum radial order per `(l, pol)` in the analytic catalog.
const N_MAX: usize = 5;

/// Result for a single benchmark row.
///
/// Each row records one FEM eigenmode together with the analytic
/// `(l, n, polarisation)` group it was claimed into and which slot
/// within the `2 l + 1` magnetic-degeneracy multiplet it occupies.
#[derive(Debug, Clone)]
struct Row {
    pol: &'static str,
    l: usize,
    n: usize,
    /// Slot within the `2 l + 1` degenerate group (0-indexed).
    m_idx: usize,
    /// True if the FEM spectrum ran out before the full multiplicity
    /// was filled.
    incomplete: bool,
    analytic_k: f64,
    fem_re_k: f64,
    fem_im_k: f64,
    rel_err_re_k: f64,
    q: f64,
}

fn pol_str(pol: MiePolarisation) -> &'static str {
    match pol {
        MiePolarisation::TE => "TE",
        MiePolarisation::TM => "TM",
    }
}

fn current_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn results_path() -> PathBuf {
    // `crates/geode-core/examples/mie_sphere.rs` → walk up 3 levels to
    // the workspace root, then into `benchmarks/mie_sphere/`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
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
/// tensor (~6% rel err on TM_1,1, issue #54).
fn fem_complex_k(use_dense: bool, scalar_pml: bool) -> Vec<faer::c64> {
    let device = <B as BackendTypes>::Device::default();

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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device);
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
            "PML kernel: anisotropic UPML diagonal complex ε (default, issue #54; k₀_ref = {K0_REF}; expect ~6% TM_1,1 rel err)"
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
    let n_request = spurious_dim + N_MODES + 5;

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
        // generalized_eigen, but for the larger refined meshes it
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
            sigma: 0.0,
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

    // Spurious filter: anything with |λ| below 1e-3 of the largest
    // requested |λ| is treated as gradient-kernel noise.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
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
        .take(N_MODES)
        .map(|lam| {
            // Principal branch of sqrt: Re(k) ≥ 0.
            let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
            let re_k = ((r + lam.re) / 2.0).sqrt();
            let im_k_mag = ((r - lam.re) / 2.0).sqrt();
            let im_k = if lam.im >= 0.0 { im_k_mag } else { -im_k_mag };
            faer::c64::new(re_k, im_k)
        })
        .collect()
}

/// Compute the Q factor of a complex `k`: `Q = Re(k) / (2 |Im(k)|)`.
fn q_factor(k: faer::c64) -> f64 {
    if k.im.abs() > 1e-12 {
        k.re / (2.0 * k.im.abs())
    } else {
        f64::INFINITY
    }
}

/// Multiplicity-aware mode-claim pairing (issue #40).
///
/// Walks the analytic catalog in ascending-`k` order. For each root,
/// claims the next `multiplicity = 2 l + 1` FEM modes (in sorted `Re(k)`
/// order) and labels each one with its slot index `m_idx ∈ [0, 2l]`.
///
/// If fewer than `multiplicity` FEM modes remain when a group is
/// reached, the row is still emitted with `incomplete = true`. This
/// is informational, never panicking.
///
/// Im(k) banding tiebreaker is currently a soft sanity check: within
/// each claimed group we record the median `Im(k)` and warn if the
/// per-slot spread exceeds ~10 % of that median. The FEM coarse
/// fixture often violates this band (mesh asymmetry), so the message
/// is logged but not enforced.
fn pair_modes(analytic: &[MieRoot], fem: &[faer::c64]) -> Vec<Row> {
    // Sort FEM modes by Re(k) so consecutive claims are well-defined.
    let mut fem_sorted: Vec<faer::c64> = fem.to_vec();
    fem_sorted.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap());

    let mut rows = Vec::new();
    let mut cursor = 0_usize;

    for root in analytic {
        if cursor >= fem_sorted.len() {
            break;
        }
        let mult = root.multiplicity;
        let take = mult.min(fem_sorted.len() - cursor);
        let group = &fem_sorted[cursor..cursor + take];
        let incomplete = take < mult;

        // Im(k) banding diagnostic: report the relative spread of the
        // damping across this claimed multiplet.
        if group.len() >= 2 {
            let ims: Vec<f64> = group.iter().map(|k| k.im.abs()).collect();
            let im_min = ims.iter().cloned().fold(f64::INFINITY, f64::min);
            let im_max = ims.iter().cloned().fold(0.0_f64, f64::max);
            let band = if im_min > 0.0 {
                (im_max - im_min) / im_min
            } else {
                f64::INFINITY
            };
            if band > 0.10 {
                eprintln!(
                    "  note: {}_{},{} multiplet Im(k) band = {:.1}% > 10% (mesh asymmetry)",
                    pol_str(root.pol),
                    root.l,
                    root.n,
                    band * 100.0
                );
            }
        }

        for (slot, fem_k) in group.iter().enumerate() {
            let rel_err = (fem_k.re - root.k).abs() / root.k;
            rows.push(Row {
                pol: pol_str(root.pol),
                l: root.l,
                n: root.n,
                m_idx: slot,
                incomplete,
                analytic_k: root.k,
                fem_re_k: fem_k.re,
                fem_im_k: fem_k.im,
                rel_err_re_k: rel_err,
                q: q_factor(*fem_k),
            });
        }

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
        let suffix = if r.incomplete { "*" } else { " " };
        eprintln!(
            "{:>3}  {:>11}{}  {:>4}  {:>11.5}  {:>11.5}  {:>11.5e}  {:>12.3}%  {:>10.3e}",
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
    eprintln!();
}

fn write_toml(rows: &[Row], path: &PathBuf, scalar_pml: bool) {
    let commit = current_commit();
    let pml_kind = if scalar_pml { "scalar" } else { "anisotropic" };

    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    s.push_str("#   --example mie_sphere`.\n");
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Consumed by `tests/mie_sphere.rs` and the README cross-link.\n");
    s.push('\n');

    s.push_str("[meta]\n");
    s.push_str("description = \"Mie sphere benchmark (issue #4 v1, issue #40, issue #54): FEM eigenmodes vs. extended analytic PEC-cavity catalog (l ∈ [1,4], TE+TM, n ∈ [1,5]) with multiplicity-claim mode classification.\"\n");
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("pml_kernel = \"{pml_kind}\"\n"));
    s.push_str(&format!("n_inside = {}\n", N_INSIDE));
    s.push_str(&format!("sigma_0 = {}\n", SIGMA_0));
    if !scalar_pml {
        s.push_str(&format!("k0_ref = {}\n", K0_REF));
    }
    s.push_str(&format!("r_sphere = {}\n", R_SPHERE));
    s.push_str(&format!("r_buffer = {}\n", R_BUFFER));
    s.push_str(&format!("n_modes = {}\n", N_MODES));
    s.push_str(&format!("l_max = {}\n", L_MAX));
    s.push_str(&format!("n_max_radial = {}\n", N_MAX));
    s.push_str("notes = [\n");
    s.push_str(
        "  \"Analytic side is PEC-cavity dielectric resonator, not open-space Mie WGM.\",\n",
    );
    s.push_str("  \"Real analytic roots only; complex open-space Mie roots are a separate axis (#33).\",\n");
    s.push_str("  \"Mode classification: walk catalog by ascending k, claim 2l+1 FEM modes per analytic root.\",\n");
    if scalar_pml {
        s.push_str(
            "  \"FEM side: scalar isotropic PML (legacy --scalar-pml path), bundled 774-node refined fixture (issue #49). Has ~16% h-independent reflection ceiling on TM_1,1.\",\n",
        );
    } else {
        s.push_str(
            "  \"FEM side: anisotropic UPML diagonal complex permittivity tensor (default, issue #54), bundled 774-node refined fixture (issue #49). Breaks the 16% scalar ceiling — TM_1,1 ~6% rel err.\",\n",
        );
        s.push_str(
            "  \"For s_r = s_t = 1 - jσ/ω the off-diagonal rotation terms are identically zero, so the diagonal-only tensor is mathematically exact (not an approximation) for this profile.\",\n",
        );
    }
    s.push_str("  \"Driven scattering benchmark (Q_ext vs. ka) is v2 (separate scope).\",\n");
    s.push_str("]\n");
    s.push('\n');

    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!("[mode_{i}]\n"));
        s.push_str(&format!("polarisation = \"{}\"\n", r.pol));
        s.push_str(&format!("l = {}\n", r.l));
        s.push_str(&format!("n = {}\n", r.n));
        s.push_str(&format!("m_idx = {}\n", r.m_idx));
        s.push_str(&format!("incomplete = {}\n", r.incomplete));
        s.push_str(&format!("analytic_k = {:.15e}\n", r.analytic_k));
        s.push_str(&format!("fem_re_k = {:.15e}\n", r.fem_re_k));
        s.push_str(&format!("fem_im_k = {:.15e}\n", r.fem_im_k));
        s.push_str(&format!("rel_err_re_k = {:.15e}\n", r.rel_err_re_k));
        s.push_str(&format!("q = {:.15e}\n", r.q));
        s.push('\n');
    }

    fs::create_dir_all(path.parent().expect("results parent")).expect("mkdir");
    fs::write(path, s).expect("write results.toml");
    eprintln!("wrote {}", path.display());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let use_dense = args.iter().any(|a| a == "--dense");
    let scalar_pml = args.iter().any(|a| a == "--scalar-pml");

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
    eprintln!(
        "Analytic catalog: {} roots over l ∈ [1, {}], TE+TM, n ∈ [1, {}]",
        analytic.len(),
        L_MAX,
        N_MAX
    );
    eprintln!("Lowest 12 analytic roots (PEC-cavity dielectric resonator):");
    for r in analytic.iter().take(12) {
        eprintln!(
            "  {}_{},{}  k = {:.5}  k² = {:.5}  mult = {}",
            pol_str(r.pol),
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
    let fem_k = fem_complex_k(use_dense, scalar_pml);
    eprintln!("Lowest {} physical FEM modes (k = sqrt(λ)):", fem_k.len());
    for (i, k) in fem_k.iter().enumerate() {
        eprintln!("  mode[{i}]  k = {:.5} + {:.5e}i", k.re, k.im);
    }

    // Pair and report.
    let rows = pair_modes(&analytic, &fem_k);
    print_table(&rows);

    // Persist.
    write_toml(&rows, &results_path(), scalar_pml);

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
            pol_str(r.pol),
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
            pol_str(best.pol),
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
}
