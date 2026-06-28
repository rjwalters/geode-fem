//! Matched-UPML open-space quasi-mode benchmark (issue #213).
//!
//! The eigenmode benchmark in `examples/mie_sphere/` uses the
//! **ε-only** anisotropic UPML (issue #54): the permittivity is
//! stretched but μ stays 1, so the PML interface is impedance
//! mismatched. The mismatch reflects outgoing radiation back into the
//! cavity, which inflates the apparent quality factor of the
//! open-structure quasi-modes — the recorded TM₁,₁ Q ≈ 27 in
//! `benchmarks/mie_sphere/results.toml` vs. the analytic open-space
//! Q ≈ 1.95 from `geode_core::analytic::mie::OPEN_SPACE_WGM_TABLE_N15`.
//!
//! This benchmark assembles the eigenpencil with the **matched** (full
//! Sacks) UPML lifted into Burn assembly by PR #205 for the driven
//! path: `K(Λ⁻¹) x = k² M(ε_r·Λ) x` with the full-3×3 complex tensors
//! on *both* sides, so the shell is reflectionless to first order and
//! the quasi-mode linewidth (radiative decay into the PML) becomes
//! physical.
//!
//! # ω-freeze linearization
//!
//! Unlike the ε-only profile (parameterized by σ₀ directly), the
//! matched stretch `s = 1 − jσ(r)/ω` depends on ω, making the pencil
//! **nonlinear** in the eigenvalue. We linearize by freezing Λ at the
//! analytic root frequency of the target mode (`ω₀ = Re(k)` from the
//! open-space catalog, c = 1 natural units), solving the resulting
//! linear pencil, and optionally performing one Picard refresh:
//! re-assemble Λ at the recovered `Re(k)` and re-solve, reporting the
//! shift. The Picard shift is a direct measure of how much the
//! ω-freeze approximation matters at the achieved accuracy.
//!
//! # Mode targeting
//!
//! - **TM₁,₁** (k = 1.8807 − 0.4818j, Q ≈ 1.95) is the primary
//!   acceptance target — the claim to beat is the ε-only Q ≈ 27.
//! - **TE₁,₁** (k = 1.2590 − 0.8702j, Q ≈ 0.72) is best-effort: that
//!   broad a resonance is hard to separate from the PML continuum and
//!   is reported with an `ambiguous` flag when the complex-distance
//!   and nearest-`Re(k)` matches disagree.
//!
//! # Eigensolver
//!
//! The sparse shift-and-invert Lanczos (`complex_lanczos.rs`) with the
//! shift placed **at the analytic target** `σ = ω₀² = Re(k)²`: the
//! quasi-mode sits at complex distance `≈ |Im(λ)|` from the shift,
//! comfortably inside the Krylov window, and the gradient nullspace
//! (λ ≈ 0, at distance σ) never needs to be crawled through. The
//! dense QZ oracle (`FaerComplexEigensolver`) on the 3300-DOF reduced
//! pencil takes tens of minutes per solve and is impractical for the
//! 6-solve sweep — pass `--dense` to run it anyway for a one-off
//! cross-check.
//!
//! # Running
//!
//! ```sh
//! cargo run -p mie_open_quasimode --release
//! ```
//!
//! `--release` is required (faer 0.24 `gevd` panics under
//! debug-assertions on the `--dense` path, and the debug build is far
//! too slow for the dense assembly). Sparse-path runtime is ~1 minute
//! total on the bundled 774-node fixture.
//!
//! Writes `benchmarks/mie_sphere/open_results.toml` (sibling of the
//! ε-only `results.toml`, which is left untouched).
//!
//! This is a standalone example crate (`examples/mie_open_quasimode/`)
//! built on the `geode-app` harness, migrated from the old
//! `crates/geode-core/examples/mie_open_quasimode.rs` (Epic #398
//! Phase 3a). The physics, report output, and `open_results.toml`
//! artifact are preserved exactly; only the entry point (hand-rolled
//! `--dense` argv scan → `clap` derive + `geode_app::App`) changed.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use burn::tensor::backend::BackendTypes;
use clap::Parser;
use faer::sparse::{SparseColMat, Triplet};

use geode_app::{App, Verbosity};
use geode_core::analytic::mie::{MiePolarisation, MieRootComplex, open_space_wgm_roots_n15};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_full_tensors, burn_complex_mass_to_faer, sphere_n_interior_nodes,
    sphere_pec_interior_edges,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::testing::TestBackend;
use geode_core::driven::scattering::build_matched_upml_materials;
use geode_core::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
use geode_core::mesh::{PHYS_SPHERE_INTERIOR, R_BUFFER, SphereFixture, read_sphere_fixture};

type B = TestBackend;

/// Refractive index inside the sphere (matches the analytic catalog).
const N_INSIDE: f64 = 1.5;

/// UPML strength values for the sensitivity axis. 5.0 matches the
/// ε-only eigen benchmark; 25.0 is the driven-path calibration
/// (round-trip continuum attenuation `exp(−2σ₀d/3) ≈ 2·10⁻⁴`).
const SIGMA_VALUES: &[f64] = &[5.0, 25.0];

/// Number of eigenvalues nearest the shift requested from the sparse
/// solver — wide enough to cover the quasi-mode multiplet plus the
/// PML-continuum modes between it and the shift.
const N_NEAR_SHIFT: usize = 60;

/// Extra eigenvalues requested above the gradient-nullspace count on
/// the `--dense` oracle path (which sorts ascending by |Re(λ)| from 0).
const N_EXTRA_DENSE: usize = 80;

/// Principal-branch `k = sqrt(λ)` with `Re(k) ≥ 0`.
fn k_from_lambda(lam: faer::c64) -> (f64, f64) {
    let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
    let re_k = ((r + lam.re) / 2.0).sqrt();
    let im_mag = ((r - lam.re) / 2.0).sqrt();
    let im_k = if lam.im >= 0.0 { im_mag } else { -im_mag };
    (re_k, im_k)
}

/// One frozen-ω matched-UPML eigensolve: assemble
/// `K(Λ⁻¹(ω)) x = λ M(ε_rΛ(ω)) x` on the bundled fixture, PEC-reduce,
/// eigensolve (sparse shift-invert Lanczos at `σ = ω²` by default,
/// dense QZ oracle with `use_dense`), and return the physical
/// eigenvalues (gradient nullspace filtered by the magnitude-jump
/// heuristic, oscillatory `Re(λ) > 0` only), sorted by ascending
/// `Re(λ)`.
fn solve_frozen_omega(
    f: &SphereFixture,
    sigma_0: f64,
    omega: f64,
    use_dense: bool,
) -> Vec<faer::c64> {
    let device = <B as BackendTypes>::Device::default();

    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (eps_tensor, nu_tensor) = build_matched_upml_materials(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        sigma_0,
        omega,
    );

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device);
    let sys = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );

    let k_full = burn_complex_mass_to_faer(sys.k_re, sys.k_im);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // PEC outer-wall reduction (complex K — extract the interior
    // submatrices directly).
    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let k_int = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        k_full[(interior_idx[i], interior_idx[j])]
    });
    let m_int = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_full[(interior_idx[i], interior_idx[j])]
    });

    let lambdas = if use_dense {
        // One-off QZ oracle (`--dense`): sorts ascending by |Re(λ)|
        // from 0, so it must crawl through the gradient nullspace.
        let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
        let n_request = spurious_dim + N_EXTRA_DENSE;
        eprintln!(
            "  σ₀ = {sigma_0}, ω = {omega:.4}: {n_edges} edges → {dim} interior DOFs, \
             dense QZ requesting {n_request} eigenvalues"
        );
        FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(k_int.as_ref(), m_int.as_ref(), n_request)
            .expect("dense complex eigensolve")
    } else {
        // Default: sparse shift-invert Lanczos at σ = ω² (the
        // frozen-Λ frequency). The shift puts the gradient nullspace
        // λ ≈ 0 at distance σ, so no spurious-mode filter is needed
        // beyond the oscillatory cut below.
        eprintln!(
            "  σ₀ = {sigma_0}, ω = {omega:.4}: {n_edges} edges → {dim} interior DOFs, \
             requesting {N_NEAR_SHIFT} eigenvalues near σ = {:.4}",
            omega * omega
        );
        let mut k_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
        let mut m_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
        for j in 0..dim {
            for i in 0..dim {
                let kv = k_int[(i, j)];
                if kv.re != 0.0 || kv.im != 0.0 {
                    k_trips.push(Triplet::new(i, j, kv));
                }
                let mv = m_int[(i, j)];
                if mv.re != 0.0 || mv.im != 0.0 {
                    m_trips.push(Triplet::new(i, j, mv));
                }
            }
        }
        let k_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &k_trips)
            .expect("sparse K");
        let m_sp = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &m_trips)
            .expect("sparse M");
        SparseComplexShiftInvertLanczos {
            sigma: omega * omega,
            max_iters: 256,
            tol: 1e-9,
        }
        .smallest_complex_pencil_eigenvalues(k_sp.as_ref(), m_sp.as_ref(), N_NEAR_SHIFT)
        .expect("sparse shift-invert complex eigensolve")
    };

    // Gradient-nullspace filter (no-op on the shift-invert path) +
    // oscillatory only.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let thresh = if use_dense { 1e-3 * max_abs } else { 0.0 };
    let mut physical: Vec<faer::c64> = lambdas
        .iter()
        .filter(|l| l.re.hypot(l.im) > thresh && l.re > 0.0)
        .copied()
        .collect();
    physical.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap());
    physical
}

/// Match the physical spectrum against an analytic complex root:
/// minimize the complex distance `hypot(Re k − Re k_a, |Im k| − |Im k_a|)`
/// in the k-plane (|Im| folds out the time-convention sign). Returns
/// `(λ_best, ambiguous)` where `ambiguous` flags disagreement with the
/// nearest-`Re(k)` match.
fn match_root(physical: &[faer::c64], root: &MieRootComplex) -> Option<(faer::c64, bool)> {
    let dist = |lam: &faer::c64| {
        let (re_k, im_k) = k_from_lambda(*lam);
        (re_k - root.re_k).hypot(im_k.abs() - root.im_k.abs())
    };
    let best = physical
        .iter()
        .min_by(|a, b| dist(a).partial_cmp(&dist(b)).unwrap())?;
    let nearest_re = physical
        .iter()
        .min_by(|a, b| {
            let da = (k_from_lambda(**a).0 - root.re_k).abs();
            let db = (k_from_lambda(**b).0 - root.re_k).abs();
            da.partial_cmp(&db).unwrap()
        })
        .expect("non-empty if best matched");
    let ambiguous = (best.re - nearest_re.re).abs() > 1e-12 * best.re.abs().max(1.0)
        || (best.im - nearest_re.im).abs() > 1e-12 * best.im.abs().max(1.0);
    Some((*best, ambiguous))
}

struct QuasiModeRow {
    sigma_0: f64,
    pol: MiePolarisation,
    l: usize,
    n: usize,
    omega_freeze: f64,
    analytic_re_k: f64,
    analytic_im_k: f64,
    analytic_q: f64,
    fem_re_k: f64,
    fem_im_k: f64,
    fem_q: f64,
    rel_err_re_k: f64,
    q_ratio: f64,
    ambiguous: bool,
    /// One Picard refresh (re-freeze Λ at the recovered `Re(k)`,
    /// re-solve): `(ω₁, Re(k), Im(k), Q)`.
    picard: Option<(f64, f64, f64, f64)>,
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmarks")
        .join("mie_sphere")
        .join("open_results.toml")
}

fn write_results(rows: &[QuasiModeRow]) {
    let path = results_path();
    let mut s = String::new();
    s.push_str(
        "# Auto-generated by `cargo run -p geode-core --release \\\n\
         #   --example mie_open_quasimode`.\n\
         # Do NOT edit by hand — regenerate after any intentional change.\n\
         # Consumed by `tests/sphere_matched_upml_eigenmode.rs` and the\n\
         # issue #213 acceptance record.\n\n",
    );
    s.push_str("[meta]\n");
    s.push_str(
        "description = \"Matched (full Sacks) UPML quasi-mode benchmark (issue #213): \
         frozen-ω complex eigenpencil vs. open-space Mie WGM complex roots \
         (OPEN_SPACE_WGM_TABLE_N15).\"\n",
    );
    s.push_str(&format!("generated_at_commit = \"{}\"\n", current_commit()));
    s.push_str("pml_kernel = \"matched_full_sacks\"\n");
    s.push_str(&format!("n_inside = {N_INSIDE}\n"));
    s.push_str(&format!("sigma_values = {SIGMA_VALUES:?}\n"));
    s.push_str("notes = [\n");
    s.push_str(
        "  \"Pencil: K(Λ⁻¹(ω₀)) x = k² M(ε_r·Λ(ω₀)) x with Λ frozen at the analytic \
         root frequency ω₀ = Re(k); one Picard refresh at the recovered Re(k) is \
         reported per row where present.\",\n",
    );
    s.push_str(
        "  \"Claim to beat: ε-only UPML TM_1,1 Q ≈ 27 (results.toml) vs analytic \
         open-space Q ≈ 1.95.\",\n",
    );
    s.push_str(
        "  \"TE_1,1 (analytic Q ≈ 0.72) is best-effort: that broad a resonance \
         competes with the PML continuum; rows flagged ambiguous when the \
         complex-distance and nearest-Re(k) matches disagree.\",\n",
    );
    s.push_str(
        "  \"Residual gap to the analytic root is dominated by PML truncation \
         (thin shell at finite R_b) + discretization on the bundled 774-node \
         fixture, not by the impedance mismatch the ε-only kernel had.\",\n",
    );
    s.push_str("]\n\n");

    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!("[quasimode_{i}]\n"));
        s.push_str(&format!("sigma_0 = {}\n", r.sigma_0));
        s.push_str(&format!("polarisation = \"{}\"\n", pol_str(r.pol)));
        s.push_str(&format!("l = {}\n", r.l));
        s.push_str(&format!("n = {}\n", r.n));
        s.push_str(&format!("omega_freeze = {:.15e}\n", r.omega_freeze));
        s.push_str(&format!("analytic_re_k = {:.15e}\n", r.analytic_re_k));
        s.push_str(&format!("analytic_im_k = {:.15e}\n", r.analytic_im_k));
        s.push_str(&format!("analytic_q = {:.15e}\n", r.analytic_q));
        s.push_str(&format!("fem_re_k = {:.15e}\n", r.fem_re_k));
        s.push_str(&format!("fem_im_k = {:.15e}\n", r.fem_im_k));
        s.push_str(&format!("fem_q = {:.15e}\n", r.fem_q));
        s.push_str(&format!("rel_err_re_k = {:.15e}\n", r.rel_err_re_k));
        s.push_str(&format!("q_ratio = {:.15e}\n", r.q_ratio));
        s.push_str(&format!("ambiguous = {}\n", r.ambiguous));
        if let Some((w1, re_k, im_k, q)) = r.picard {
            s.push_str(&format!("picard_omega = {w1:.15e}\n"));
            s.push_str(&format!("picard_re_k = {re_k:.15e}\n"));
            s.push_str(&format!("picard_im_k = {im_k:.15e}\n"));
            s.push_str(&format!("picard_q = {q:.15e}\n"));
        }
        s.push('\n');
    }

    fs::create_dir_all(path.parent().expect("results parent")).expect("mkdir");
    fs::write(&path, s).expect("write open_results.toml");
    eprintln!("wrote {}", path.display());
}

/// Matched-UPML open-space quasi-mode benchmark CLI.
///
/// Flattens the shared `geode-app` `-v`/`-q` verbosity group and keeps
/// the example-local `--dense` toggle the original hand-rolled argv scan
/// recognised.
#[derive(Parser)]
#[command(
    about = "Matched (full Sacks) UPML open-space quasi-mode benchmark vs. Mie WGM complex roots (issue #213)."
)]
struct Args {
    /// Use the dense `FaerComplexEigensolver` QZ oracle (tens of minutes
    /// per solve) instead of the default sparse shift-invert Lanczos.
    #[arg(long)]
    dense: bool,

    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let use_dense = self.dense;
        let f = read_sphere_fixture()?;
        eprintln!(
            "sphere fixture: {} nodes, {} tets",
            f.mesh.n_nodes(),
            f.mesh.n_tets()
        );

        let catalog = open_space_wgm_roots_n15();
        // Primary acceptance target TM₁,₁ first, then best-effort TE₁,₁.
        let targets: Vec<&MieRootComplex> = [
            (MiePolarisation::TM, 1usize, 1usize),
            (MiePolarisation::TE, 1, 1),
        ]
        .iter()
        .map(|&(pol, l, n)| {
            catalog
                .iter()
                .find(|r| r.pol == pol && r.l == l && r.n == n)
                .expect("target root in catalog")
        })
        .collect();

        let mut rows = Vec::new();
        for &sigma_0 in SIGMA_VALUES {
            for root in &targets {
                let omega0 = root.re_k;
                eprintln!(
                    "=== σ₀ = {sigma_0}, target {}_{},{} (analytic k = {:.4} {:+.4}j, Q = {:.3}) ===",
                    pol_str(root.pol),
                    root.l,
                    root.n,
                    root.re_k,
                    root.im_k,
                    root.q()
                );
                let physical = solve_frozen_omega(&f, sigma_0, omega0, use_dense);
                eprintln!("  {} oscillatory physical modes", physical.len());
                let Some((lam, ambiguous)) = match_root(&physical, root) else {
                    eprintln!("  no physical mode matched — skipping row");
                    continue;
                };
                let (re_k, im_k) = k_from_lambda(lam);
                let q = if im_k.abs() > 1e-12 {
                    re_k / (2.0 * im_k.abs())
                } else {
                    f64::INFINITY
                };
                eprintln!(
                    "  matched quasi-mode: k = {re_k:.4} {im_k:+.4}j, Q = {q:.3} \
                 (rel err Re(k) = {:.2}%, Q ratio = {:.3}{})",
                    (re_k - root.re_k).abs() / root.re_k * 100.0,
                    q / root.q(),
                    if ambiguous { ", AMBIGUOUS" } else { "" }
                );

                // One Picard refresh for the primary TM target: re-freeze Λ
                // at the recovered Re(k) and re-solve.
                let picard = if root.pol == MiePolarisation::TM {
                    let omega1 = re_k;
                    eprintln!("  Picard refresh at ω₁ = {omega1:.4} …");
                    let physical1 = solve_frozen_omega(&f, sigma_0, omega1, use_dense);
                    match_root(&physical1, root).map(|(lam1, _)| {
                        let (re1, im1) = k_from_lambda(lam1);
                        let q1 = if im1.abs() > 1e-12 {
                            re1 / (2.0 * im1.abs())
                        } else {
                            f64::INFINITY
                        };
                        eprintln!(
                            "  Picard: k = {re1:.4} {im1:+.4}j, Q = {q1:.3} \
                         (ΔRe(k) = {:+.2e}, ΔQ = {:+.3})",
                            re1 - re_k,
                            q1 - q
                        );
                        (omega1, re1, im1, q1)
                    })
                } else {
                    None
                };

                rows.push(QuasiModeRow {
                    sigma_0,
                    pol: root.pol,
                    l: root.l,
                    n: root.n,
                    omega_freeze: omega0,
                    analytic_re_k: root.re_k,
                    analytic_im_k: root.im_k,
                    analytic_q: root.q(),
                    fem_re_k: re_k,
                    fem_im_k: im_k,
                    fem_q: q,
                    rel_err_re_k: (re_k - root.re_k).abs() / root.re_k,
                    q_ratio: q / root.q(),
                    ambiguous,
                    picard,
                });
            }
        }

        write_results(&rows);

        eprintln!("\nSummary (vs. open-space analytic roots):");
        for r in &rows {
            eprintln!(
                "  σ₀ = {:>4}: {}_{},{}  Re(k) {:.4} vs {:.4} ({:.1}% err), \
             Q {:.3} vs {:.3} (ratio {:.3}){}",
                r.sigma_0,
                pol_str(r.pol),
                r.l,
                r.n,
                r.fem_re_k,
                r.analytic_re_k,
                r.rel_err_re_k * 100.0,
                r.fem_q,
                r.analytic_q,
                r.q_ratio,
                if r.ambiguous { " [ambiguous]" } else { "" }
            );
        }

        Ok(())
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<Args>()
}
