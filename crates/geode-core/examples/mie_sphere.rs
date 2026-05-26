//! Mie-sphere benchmark — FEM eigenmodes vs. analytic PEC-cavity
//! dielectric-sphere resonance roots (issue #4, north-star deliverable).
//!
//! This is the **v0** cut: the analytic ground truth is the
//! PEC-outer-wall dielectric resonator (`geode_core::mie`), and the
//! FEM result is the scalar-PML eigenspectrum produced by
//! `assemble_global_nedelec_with_complex_epsilon` on the bundled
//! sphere fixture (the same physical setup as `tests/sphere_pml_*`).
//!
//! # Honest scope
//!
//! - **Analytic side**: real-only PEC-cavity roots, NOT the
//!   complex open-space Mie WGM positions. The latter require Hankel
//!   functions and complex Newton iteration; v1.
//! - **FEM side**: 313-node tet mesh (the bundled fixture), scalar
//!   isotropic PML over the vacuum buffer, σ₀ = 5.0. This is coarse:
//!   expect ~20-50 % relative error in `Re(k)` for the lowest mode
//!   at this resolution and PML strength.
//! - **Driven scattering** (Q_ext, Q_sca vs. ka) is **v1** — see
//!   issue #4 owner comment of 2026-05-26.
//!
//! The point of v0 is to **wire up the full comparison pipeline** in
//! one program, write the table out, and have a starting baseline.
//! Quantitative tightening lives in follow-up issues (#33, #35, #38).
//!
//! # Running
//!
//! ```sh
//! cargo run -p geode-core --release --example mie_sphere
//! ```
//!
//! `--release` is required because faer 0.24's `gevd` path panics
//! under `debug-assertions` (same root cause as `tests/sphere_pml_*`).
//!
//! Writes `benchmarks/mie_sphere/results.toml` relative to the
//! workspace root (located via `CARGO_MANIFEST_DIR`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use burn::tensor::backend::BackendTypes;

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, burn_matrix_to_faer, merged_roots, read_sphere_fixture,
    sphere_n_interior_nodes, sphere_pec_interior_edges, tet_centroid_radii, upload_mesh,
    ComplexEigenSolver, DefaultBackend, FaerComplexEigensolver, MiePolarisation, MieRoot, R_BUFFER,
    R_SPHERE,
};

type B = DefaultBackend;

/// Refractive index inside the sphere. n=1.5 is the textbook
/// B&H dielectric test case.
const N_INSIDE: f64 = 1.5;

/// PML absorption strength. σ₀ = 5.0 matches `tests/sphere_pml_*`.
const SIGMA_0: f64 = 5.0;

/// Number of physical FEM modes to compare (above the gradient nullspace).
const N_MODES: usize = 5;

/// Result for a single benchmark row.
#[derive(Debug, Clone)]
struct Row {
    pol: &'static str,
    l: usize,
    n: usize,
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
fn fem_complex_k() -> Vec<faer::c64> {
    let device = <B as BackendTypes>::Device::default();

    let f = read_sphere_fixture().expect("fixture load");
    eprintln!(
        "sphere fixture: {} nodes, {} tets, {} boundary triangles",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.boundary_triangles.len(),
    );

    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);

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
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex,
    );

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

    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

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

fn pair_modes(analytic: &[MieRoot], fem: &[faer::c64]) -> Vec<Row> {
    let mut rows = Vec::new();
    for fem_k in fem {
        // Find closest analytic root by |Re(k)|.
        let (idx, best) = analytic
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let da = (a.1.k - fem_k.re).abs();
                let db = (b.1.k - fem_k.re).abs();
                da.partial_cmp(&db).unwrap()
            })
            .expect("at least one analytic root");
        let _ = idx;
        let rel_err = (fem_k.re - best.k).abs() / best.k;
        let q = if fem_k.im.abs() > 1e-12 {
            fem_k.re / (2.0 * fem_k.im.abs())
        } else {
            f64::INFINITY
        };
        rows.push(Row {
            pol: pol_str(best.pol),
            l: best.l,
            n: best.n,
            analytic_k: best.k,
            fem_re_k: fem_k.re,
            fem_im_k: fem_k.im,
            rel_err_re_k: rel_err,
            q,
        });
    }
    rows
}

fn print_table(rows: &[Row]) {
    eprintln!();
    eprintln!(
        "{:>3}  {:>9}  {:>11}  {:>11}  {:>11}  {:>12}  {:>10}",
        "i", "mode", "analytic k", "FEM Re(k)", "FEM Im(k)", "rel err Re(k)", "Q"
    );
    eprintln!("{}", "-".repeat(3 + 9 + 11 + 11 + 11 + 12 + 10 + 6 * 2));
    for (i, r) in rows.iter().enumerate() {
        eprintln!(
            "{:>3}  {:>3}_{},{:<3}  {:>11.5}  {:>11.5}  {:>11.5e}  {:>12.3}%  {:>10.3e}",
            i,
            r.pol,
            r.l,
            r.n,
            r.analytic_k,
            r.fem_re_k,
            r.fem_im_k,
            r.rel_err_re_k * 100.0,
            r.q,
        );
    }
    eprintln!();
}

fn write_toml(rows: &[Row], path: &PathBuf) {
    let commit = current_commit();

    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    s.push_str("#   --example mie_sphere`.\n");
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Consumed by `tests/mie_sphere.rs` and the README cross-link.\n");
    s.push('\n');

    s.push_str("[meta]\n");
    s.push_str("description = \"Mie sphere benchmark (issue #4 v0): FEM eigenmodes vs. analytic PEC-cavity dielectric-sphere resonance roots.\"\n");
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("n_inside = {}\n", N_INSIDE));
    s.push_str(&format!("sigma_0 = {}\n", SIGMA_0));
    s.push_str(&format!("r_sphere = {}\n", R_SPHERE));
    s.push_str(&format!("r_buffer = {}\n", R_BUFFER));
    s.push_str(&format!("n_modes = {}\n", N_MODES));
    s.push_str("notes = [\n");
    s.push_str(
        "  \"v0: analytic side is PEC-cavity dielectric resonator, not open-space Mie WGM.\",\n",
    );
    s.push_str("  \"v0: real analytic roots only; complex Mie roots are v1.\",\n");
    s.push_str("  \"FEM side: scalar isotropic PML, bundled 313-node fixture — coarse.\",\n");
    s.push_str("  \"Driven scattering benchmark (Q_ext vs. ka) is v1.\",\n");
    s.push_str("]\n");
    s.push('\n');

    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!("[mode_{i}]\n"));
        s.push_str(&format!("polarisation = \"{}\"\n", r.pol));
        s.push_str(&format!("l = {}\n", r.l));
        s.push_str(&format!("n = {}\n", r.n));
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
    eprintln!("=== Mie sphere benchmark (issue #4 v0) ===");
    eprintln!();
    eprintln!(
        "Fixture geometry: R_sphere = {R_SPHERE}, R_buffer = {R_BUFFER}, n_inside = {N_INSIDE}",
    );
    eprintln!("PML absorption: σ₀ = {SIGMA_0}");
    eprintln!();

    // Analytic ground truth: lowest TE + TM roots for l ∈ {1, 2, 3}.
    // The merged list is sorted by k, so the lowest `2 * N_MODES`
    // entries cover the FEM spectrum window with margin.
    let analytic = merged_roots(N_INSIDE, &[1, 2, 3], R_SPHERE, R_BUFFER, N_MODES);
    eprintln!("Lowest analytic roots (PEC-cavity dielectric resonator):");
    for r in &analytic {
        eprintln!(
            "  {}_{},{}  k = {:.5}  k² = {:.5}",
            pol_str(r.pol),
            r.l,
            r.n,
            r.k,
            r.k * r.k
        );
    }

    // FEM eigensolve.
    eprintln!();
    eprintln!("=== FEM eigensolve ===");
    let fem_k = fem_complex_k();
    eprintln!("Lowest {} physical FEM modes (k = sqrt(λ)):", fem_k.len());
    for (i, k) in fem_k.iter().enumerate() {
        eprintln!("  mode[{i}]  k = {:.5} + {:.5e}i", k.re, k.im);
    }

    // Pair and report.
    let rows = pair_modes(&analytic, &fem_k);
    print_table(&rows);

    // Persist.
    write_toml(&rows, &results_path());

    eprintln!("=== Done ===");
}
