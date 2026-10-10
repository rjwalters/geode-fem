//! Matched-UPML open-space quasi-mode benchmark (issue #213).
//!
//! The eigenmode benchmark in `examples/mie_sphere/` uses the
//! **ε-only** anisotropic UPML (issue #54): the permittivity is
//! stretched but μ stays 1, so the PML interface is impedance
//! mismatched. The mismatch reflects outgoing radiation back into the
//! cavity, which inflates the apparent quality factor of the
//! open-structure quasi-modes. In `benchmarks/mie_sphere/results.toml`
//! the ε-only triplet nearest this benchmark's primary target
//! (`Re(k) ≈ 1.870`, next to the PEC-cavity TE₁,₁ = 1.86880) has Q ≈ 9.0,
//! vs. the analytic open-space TE₁,₁ Q ≈ 1.95 from
//! `geode_core::analytic::mie::OPEN_SPACE_WGM_TABLE_N15`. (The ε-only
//! ground triplet, `Re(k) ≈ 1.229`, Q ≈ 27, is TM₁,₁, whose analytic
//! open-space Q is ≈ 0.72.)
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
//! # Mode targeting and identity (issue #1022)
//!
//! - **TE₁,₁** (k = 1.8807 − 0.4818j, Q ≈ 1.95; the Bohren & Huffman
//!   `b₁` magnetic-dipole pole) is the primary acceptance target — the
//!   claim to beat is the ε-only Q ≈ 9.0 of the same mode.
//! - **TM₁,₁** (k = 1.2590 − 0.8702j, Q ≈ 0.72; the `a₁` electric-dipole
//!   pole) is best-effort: that broad a resonance is hard to separate
//!   from the PML continuum.
//!
//! A row is **not** the eigenvalue nearest the analytic root. Each
//! eigenvector is classified with
//! `geode_core::postproc::mode_character` (energy split ball / gap / PML,
//! radial-`E` share, projection onto the TE/TM `l = 1, 2` multipole
//! families), and a row reports the nearest **identified multiplet** of
//! the target family: at least half of the in-ball energy in that family
//! and exactly `2l + 1 = 3` such modes grouped in `k`. `identified` is
//! `false` when no complete multiplet exists; `nearest_is_member` says
//! whether the plain nearest mode belongs to the reported multiplet, and
//! `nearest_family` names what the nearest mode is when it does not.
//! These replace the `ambiguous` flag earlier copies of
//! `open_results.toml` carried.
//!
//! At `σ₀ = 25` the nearest mode to the TE₁,₁ root is a TM₁ mode
//! (`k = 1.7772 + 0.6792j`), which is what the pre-#1022 benchmark
//! reported as "TE₁,₁, Re(k) 5.5 % low". The TE₁ content there is split
//! between two triplets, 19 % high and 26 % low on `Re(k)`. The identity
//! check fixes polarisation and `l`, not the radial order, and the row
//! reports the triplet nearest the root, so at `σ₀ = 25` it reports the
//! 19 %-high one. The σ₀ continuation below (issue #1026) shows that this
//! is a separate branch and that the TE₁,₁ branch is the 26 %-low triplet.
//!
//! # σ₀ continuation (`--sigma-sweep`, issue #1026)
//!
//! `--sigma-sweep` replaces the three-point benchmark with a continuation
//! in σ₀ (default 5 → 25 in steps of 0.5; the committed table uses
//! `--sigma-step 0.25`), Λ frozen at the TE₁,₁ root frequency at every
//! step, that follows every TE₁ triplet by eigenvector-subspace overlap in
//! both directions. See the `sweep` module docs for the method. Writes
//! `benchmarks/mie_sphere/open_sigma_sweep.toml`:
//!
//! ```sh
//! cargo run -p mie_open_quasimode --release -- --sigma-sweep --sigma-step 0.25
//! ```
//!
//! Before issue #999 the open-space catalog had TE and TM swapped, so
//! these two targets were labelled the other way round (the primary was
//! called TM₁,₁) and the ε-only comparison quoted the Q ≈ 27 of the
//! TM₁,₁ ground triplet. The two roots, and which one is primary, are
//! unchanged.
//!
//! # Eigensolver
//!
//! The sparse shift-and-invert Lanczos (`complex_lanczos.rs`) with the
//! shift placed **at the analytic target** `σ = ω₀² = Re(k)²`: the
//! quasi-mode sits at complex distance `≈ |Im(λ)|` from the shift,
//! comfortably inside the Krylov window, and the gradient nullspace
//! (λ ≈ 0, at distance σ) never needs to be crawled through. The
//! dense oracle (`FaerComplexEigensolver`, dense shift-invert since
//! issue #796; faer's complex QZ it used before did not terminate on this
//! pencil) computes the whole spectrum of the 3300-DOF reduced pencil and
//! is slower than the targeted sparse solve. Pass `--dense` to run it for
//! a one-off cross-check.
//!
//! # Running
//!
//! ```sh
//! cargo run -p mie_open_quasimode --release
//! ```
//!
//! `--release` is required (faer 0.24 `gevd` panics under
//! debug-assertions on the `--dense` path, and the debug build is far
//! too slow for the dense assembly). Sparse-path runtime is about
//! 20 s total on the bundled 774-node fixture.
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

mod sweep;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use burn::prelude::Backend;
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
use geode_core::driven::scattering::build_matched_upml_materials;
use geode_core::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexShiftInvertLanczos,
};
use geode_core::eigen::lanczos::ConvergenceCheck;
use geode_core::mesh::{PHYS_SPHERE_INTERIOR, R_BUFFER, SphereFixture, read_sphere_fixture};
use geode_core::postproc::mode_character::{
    ClassifiedMode, MultipoleFamily, classify_sphere_modes, select_multiplet,
};
use geode_core::testing::TestBackend;

/// Refractive index inside the sphere (matches the analytic catalog).
const N_INSIDE: f64 = 1.5;

/// UPML strength values for the sensitivity axis. 5.0 matches the
/// ε-only eigen benchmark; 25.0 is the driven-path calibration
/// (round-trip continuum attenuation `exp(−2σ₀d/3) ≈ 2·10⁻⁴`); 10.0 sits
/// between them (added with the identity check of issue #1022, which
/// showed that 25.0 does not isolate TE₁,₁ on the eigen path).
const SIGMA_VALUES: &[f64] = &[5.0, 10.0, 25.0];

/// Number of eigenvalues nearest the shift requested from the sparse
/// solver — wide enough to cover the quasi-mode multiplet plus the
/// PML-continuum modes between it and the shift.
const N_NEAR_SHIFT: usize = 60;

/// Krylov dimension of the sparse solve's first pass; the checked solve
/// extends it (up to [`KRYLOV_CAP`]) until every requested pair has
/// converged, because the identity check needs converged eigenvectors.
const KRYLOV_DIM: usize = 128;

/// Hard cap on the extended Krylov dimension.
const KRYLOV_CAP: usize = 512;

/// Relative-residual tolerance for a pair to be classified.
const RESIDUAL_TOL: f64 = 1e-8;

/// How many of the modes nearest a target to print per solve.
const N_PRINT: usize = 8;

/// Extra eigenvalues requested above the gradient-nullspace count on
/// the `--dense` oracle path (which sorts ascending by |Re(λ)| from 0).
const N_EXTRA_DENSE: usize = 80;

/// One frozen-ω matched-UPML eigensolve: assemble
/// `K(Λ⁻¹(ω)) x = λ M(ε_rΛ(ω)) x` on the bundled fixture, PEC-reduce,
/// eigensolve with eigenvectors (sparse shift-invert Lanczos at `σ = ω²`
/// by default, with a per-pair convergence check; dense oracle with
/// `use_dense`), and return the physical modes (gradient nullspace
/// filtered by the magnitude-jump heuristic, oscillatory `Re(λ) > 0`
/// only) classified by field character.
fn solve_frozen_omega<B: Backend>(
    device: &B::Device,
    f: &SphereFixture,
    sigma_0: f64,
    omega: f64,
    use_dense: bool,
) -> Vec<ClassifiedMode> {
    solve_frozen_omega_with_vectors::<B>(device, f, sigma_0, omega, use_dense).0
}

/// [`solve_frozen_omega`], also returning each mode's full-length
/// edge-DOF eigenvector (same order as the classified modes). The σ₀
/// continuation sweep (issue #1026) follows branches by eigenvector
/// overlap, so it needs the vectors the plain benchmark drops.
pub(crate) fn solve_frozen_omega_with_vectors<B: Backend>(
    device: &B::Device,
    f: &SphereFixture,
    sigma_0: f64,
    omega: f64,
    use_dense: bool,
) -> (Vec<ClassifiedMode>, Vec<Vec<faer::c64>>) {
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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, device);
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

    let pairs: Vec<(faer::c64, Vec<faer::c64>)> = if use_dense {
        // One-off dense oracle (`--dense`): sorts ascending by |Re(λ)|
        // from 0, so it must crawl through the gradient nullspace.
        let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
        let n_request = spurious_dim + N_EXTRA_DENSE;
        eprintln!(
            "  σ₀ = {sigma_0}, ω = {omega:.4}: {n_edges} edges → {dim} interior DOFs, \
             dense oracle requesting {n_request} eigenpairs"
        );
        FaerComplexEigensolver
            .smallest_complex_pencil_pairs(k_int.as_ref(), m_int.as_ref(), n_request)
            .expect("dense complex eigensolve")
    } else {
        // Default: sparse shift-invert Lanczos at σ = ω² (the
        // frozen-Λ frequency). The shift puts the gradient nullspace
        // λ ≈ 0 at distance σ, so no spurious-mode filter is needed
        // beyond the oscillatory cut below.
        eprintln!(
            "  σ₀ = {sigma_0}, ω = {omega:.4}: {n_edges} edges → {dim} interior DOFs, \
             requesting {N_NEAR_SHIFT} eigenpairs near σ = {:.4}",
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
        let checked = SparseComplexShiftInvertLanczos {
            sigma: omega * omega,
            max_iters: KRYLOV_DIM,
            tol: 1e-9,
        }
        .smallest_eigenpairs_checked(
            k_sp.as_ref(),
            m_sp.as_ref(),
            N_NEAR_SHIFT,
            ConvergenceCheck {
                residual_tol: RESIDUAL_TOL,
                max_iters_cap: KRYLOV_CAP,
                window: None,
            },
        )
        .expect("sparse shift-invert complex eigensolve");
        eprintln!(
            "  {} converged pairs (max relative residual {:.1e}), {} withheld, {} Lanczos steps",
            checked.pairs.len(),
            checked.residuals.iter().copied().fold(0.0_f64, f64::max),
            checked.rejected.len(),
            checked.lanczos_steps
        );
        checked
            .pairs
            .into_iter()
            .map(|p| (p.lambda, p.vector))
            .collect()
    };

    // Gradient-nullspace filter (no-op on the shift-invert path) +
    // oscillatory only; scatter back to full-length edge vectors for the
    // field-character integrals.
    let max_abs = pairs
        .iter()
        .map(|(l, _)| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let thresh = if use_dense { 1e-3 * max_abs } else { 0.0 };
    let mut physical: Vec<(faer::c64, Vec<faer::c64>)> = pairs
        .into_iter()
        .filter(|(l, _)| l.re.hypot(l.im) > thresh && l.re > 0.0)
        .map(|(l, v)| {
            let mut full = vec![faer::c64::new(0.0, 0.0); n_edges];
            for (i, &g) in interior_idx.iter().enumerate() {
                full[g] = v[i];
            }
            (l, full)
        })
        .collect();
    physical.sort_by(|a, b| a.0.re.partial_cmp(&b.0.re).unwrap());
    let modes: Vec<(faer::c64, &[faer::c64])> =
        physical.iter().map(|(l, v)| (*l, v.as_slice())).collect();
    let classified = classify_sphere_modes(f, &modes, N_INSIDE);
    (classified, physical.into_iter().map(|(_, v)| v).collect())
}

/// The target family of an analytic root (`l = 1` or `2` only).
fn family_of(root: &MieRootComplex) -> MultipoleFamily {
    MultipoleFamily {
        pol: root.pol,
        l: root.l,
    }
}

/// What a solve says about one analytic root.
struct RootMatch {
    /// The nearest identified multiplet of the root's family, nearest
    /// member first; `None` if no complete multiplet was found.
    multiplet: Option<Vec<ClassifiedMode>>,
    /// The plain nearest mode in `(Re k, |Im k|)` — what this benchmark
    /// reported before issue #1022.
    nearest: ClassifiedMode,
    /// Whether `nearest` belongs to `multiplet`.
    nearest_is_member: bool,
    /// Mean `(Re k, Im k, Q)` of every complete multiplet of the family
    /// in the solve, nearest the root first.
    family_multiplets: Vec<(f64, f64, f64)>,
}

/// Match the classified spectrum against an analytic complex root by
/// **identity**: select the complete multiplet of the root's family
/// nearest the root in `hypot(Re k − Re k_a, |Im k| − |Im k_a|)` (|Im|
/// folds out the time-convention sign). Prints the audit table of the
/// [`N_PRINT`] nearest modes. `None` for an empty spectrum.
fn match_root(modes: &[ClassifiedMode], root: &MieRootComplex) -> Option<RootMatch> {
    let family = family_of(root);
    let dist = |m: &ClassifiedMode| m.distance_to(root.re_k, root.im_k);
    let mut order: Vec<usize> = (0..modes.len()).collect();
    order.sort_by(|a, b| dist(&modes[*a]).partial_cmp(&dist(&modes[*b])).unwrap());
    let nearest_idx = *order.first()?;
    for &i in order.iter().take(N_PRINT) {
        let m = &modes[i];
        eprintln!(
            "    dist = {:.4}  {}  ⇒ {}",
            dist(m),
            m.summary(),
            match m.rejection(family) {
                None => format!("{} multiplet member", family.label()),
                Some(why) => format!("not {}: {why}", family.label()),
            }
        );
    }
    let members = select_multiplet(modes, family, root.re_k, root.im_k);
    let nearest_is_member = members
        .as_ref()
        .is_some_and(|idx| idx.contains(&nearest_idx));

    // Every complete multiplet of the family, by distance of its nearest
    // member.
    let mut seen: Vec<usize> = Vec::new();
    let mut family_multiplets = Vec::new();
    for &i in &order {
        let m = &modes[i];
        let Some(id) = m.multiplet_id else { continue };
        if !m.is_member_of(family) || seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let group: Vec<&ClassifiedMode> = modes
            .iter()
            .filter(|o| o.multiplet_id == Some(id))
            .collect();
        let n = group.len() as f64;
        family_multiplets.push((
            group.iter().map(|o| o.k.re).sum::<f64>() / n,
            group.iter().map(|o| o.k.im).sum::<f64>() / n,
            group.iter().map(|o| o.q()).sum::<f64>() / n,
        ));
    }

    Some(RootMatch {
        multiplet: members.map(|idx| idx.iter().map(|&i| modes[i]).collect()),
        nearest: modes[nearest_idx],
        nearest_is_member,
        family_multiplets,
    })
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
    /// The identity-based match (see [`match_root`]).
    matched: RootMatch,
    /// One Picard refresh (re-freeze Λ at the recovered `Re(k)`,
    /// re-solve, re-identify): `(ω₁, Re(k), Im(k), Q)` of the nearest
    /// member of the identified multiplet. `None` when not attempted or
    /// when the refreshed solve has no complete multiplet.
    picard: Option<(f64, f64, f64, f64)>,
}

impl QuasiModeRow {
    /// The reported mode: nearest member of the identified multiplet.
    fn reported(&self) -> Option<&ClassifiedMode> {
        self.matched.multiplet.as_ref().and_then(|m| m.first())
    }
}

/// `label_l` family name of a mode for the TOML, `"none"` if no family
/// reaches a majority.
fn family_label(m: &ClassifiedMode) -> String {
    m.family.map_or_else(|| "none".to_string(), |f| f.label())
}

fn results_path() -> PathBuf {
    geode_util::repo::repo_root()
        .join("benchmarks")
        .join("mie_sphere")
        .join("open_results.toml")
}

fn write_results(rows: &[QuasiModeRow]) {
    let path = results_path();
    let mut s = String::new();
    s.push_str(
        "# Auto-generated by `cargo run -p mie_open_quasimode --release`.\n\
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
    s.push_str(&format!(
        "generated_at_commit = \"{}\"\n",
        geode_util::repo::current_commit()
    ));
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
        "  \"Claim to beat: ε-only UPML Q ≈ 9.0 on the triplet nearest TE_1,1 \
         (Re(k) ≈ 1.870, results.toml) vs analytic open-space TE_1,1 Q ≈ 1.95. \
         The ε-only Q ≈ 27 ground triplet is TM_1,1 (analytic Q ≈ 0.72).\",\n",
    );
    s.push_str(
        "  \"Mode identity (issue #1022): fem_* is the nearest member of the nearest \
         IDENTIFIED multiplet of the target family (>= 0.5 of the in-ball energy in the \
         family's vector spherical harmonics, and exactly 2l+1 such modes grouped within \
         |dk| <= 0.02), not the eigenvalue nearest the root. identified = false means no \
         complete multiplet exists in the solve. nearest_* describes the plain nearest \
         mode, which earlier copies of this file reported (with an ambiguous flag).\",\n",
    );
    s.push_str(
        "  \"Verdict at sigma_0 = 25: the mode nearest the TE_1,1 root \
         (k = 1.7772 + 0.6792j, formerly reported as TE_1,1 with Re(k) 5.5 % low and \
         Q = 1.31) is a TM_1 mode. The 5.5 % offset was a mis-pick. The TE_1 content is \
         split between two triplets (family_multiplets_*), about 19 % high and 26 % low \
         on Re(k): sigma_0 = 25 does not isolate TE_1,1 on the 774-node fixture. The \
         Picard refresh there re-freezes at the 19 %-high triplet and the nearest TE_1 \
         triplet of the refreshed pencil is a different one (picard_re_k ≈ 1.53), so the \
         frozen-ω linearization is not self-consistent at sigma_0 = 25 either.\",\n",
    );
    s.push_str(
        "  \"Which sigma_0 = 25 triplet is TE_1,1 (issue #1026, open_sigma_sweep.toml): the \
         identity check fixes polarisation and l, not the radial order, and the row at \
         sigma_0 = 25 reports the TE_1 triplet NEAREST the root, which is the 19 %-high one. \
         The sigma_0 continuation shows that triplet is a separate branch (continuous from \
         sigma_0 = 6, where it is 51 % high), and that the sigma_0 = 5 TE_1,1 triplet continues \
         into the 26 %-low one (k ≈ 1.386 + 0.666j, Q ≈ 1.04) through a crossing with a TM_2 \
         quintuplet at sigma_0 ≈ 12.75 – 16.75 where per-mode identity is undecidable. The \
         sigma_0 = 25 fem_* values below are therefore not TE_1,1.\",\n",
    );
    s.push_str(
        "  \"TM_1,1 (analytic Q ≈ 0.72) is best-effort: that broad a resonance \
         competes with the PML continuum.\",\n",
    );
    s.push_str(
        "  \"Labels follow the issue #999 open-space catalog (TM = a_l electric \
         multipole, TE = b_l magnetic multipole); earlier copies of this file \
         had TE and TM swapped on the same two roots.\",\n",
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
        s.push_str(&format!("polarisation = \"{}\"\n", r.pol.as_str()));
        s.push_str(&format!("l = {}\n", r.l));
        s.push_str(&format!("n = {}\n", r.n));
        s.push_str(&format!("omega_freeze = {:.15e}\n", r.omega_freeze));
        s.push_str(&format!("analytic_re_k = {:.15e}\n", r.analytic_re_k));
        s.push_str(&format!("analytic_im_k = {:.15e}\n", r.analytic_im_k));
        s.push_str(&format!("analytic_q = {:.15e}\n", r.analytic_q));
        let family = MultipoleFamily { pol: r.pol, l: r.l };
        s.push_str(&format!("identified = {}\n", r.reported().is_some()));
        if let (Some(m), Some(group)) = (r.reported(), r.matched.multiplet.as_ref()) {
            let fold = |f: &dyn Fn(&ClassifiedMode) -> f64, init: f64, op: fn(f64, f64) -> f64| {
                group.iter().map(f).fold(init, op)
            };
            s.push_str(&format!("fem_re_k = {:.15e}\n", m.k.re));
            s.push_str(&format!("fem_im_k = {:.15e}\n", m.k.im));
            s.push_str(&format!("fem_q = {:.15e}\n", m.q()));
            s.push_str(&format!(
                "rel_err_re_k = {:.15e}\n",
                (m.k.re - r.analytic_re_k).abs() / r.analytic_re_k
            ));
            s.push_str(&format!("q_ratio = {:.15e}\n", m.q() / r.analytic_q));
            s.push_str(&format!("multiplet_size = {}\n", group.len()));
            s.push_str(&format!(
                "multiplet_re_k_min = {:.15e}\n",
                fold(&|m| m.k.re, f64::INFINITY, f64::min)
            ));
            s.push_str(&format!(
                "multiplet_re_k_max = {:.15e}\n",
                fold(&|m| m.k.re, f64::NEG_INFINITY, f64::max)
            ));
            s.push_str(&format!(
                "multiplet_q_min = {:.15e}\n",
                fold(&|m| m.q(), f64::INFINITY, f64::min)
            ));
            s.push_str(&format!(
                "multiplet_q_max = {:.15e}\n",
                fold(&|m| m.q(), f64::NEG_INFINITY, f64::max)
            ));
            s.push_str(&format!(
                "family_overlap_min = {:.6}\n",
                fold(&|m| m.character.overlap(family), f64::INFINITY, f64::min)
            ));
            s.push_str(&format!(
                "radial_e_fraction_max = {:.6}\n",
                fold(
                    &|m| m.character.radial_fraction,
                    f64::NEG_INFINITY,
                    f64::max
                )
            ));
            s.push_str(&format!(
                "ball_energy_fraction = {:.6}\n",
                m.character.ball_energy_fraction
            ));
            s.push_str(&format!(
                "pml_energy_fraction = {:.6}\n",
                m.character.pml_energy_fraction
            ));
        }
        let fm = &r.matched.family_multiplets;
        let list = |f: &dyn Fn(&(f64, f64, f64)) -> f64| {
            fm.iter()
                .map(|t| format!("{:.6}", f(t)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        s.push_str(&format!("family_multiplets_re_k = [{}]\n", list(&|t| t.0)));
        s.push_str(&format!("family_multiplets_im_k = [{}]\n", list(&|t| t.1)));
        s.push_str(&format!("family_multiplets_q = [{}]\n", list(&|t| t.2)));
        let near = &r.matched.nearest;
        s.push_str(&format!(
            "nearest_is_member = {}\n",
            r.matched.nearest_is_member
        ));
        s.push_str(&format!("nearest_family = \"{}\"\n", family_label(near)));
        s.push_str(&format!("nearest_re_k = {:.15e}\n", near.k.re));
        s.push_str(&format!("nearest_im_k = {:.15e}\n", near.k.im));
        s.push_str(&format!("nearest_q = {:.15e}\n", near.q()));
        s.push_str(&format!(
            "nearest_family_overlap = {:.6}\n",
            near.character.overlap(family)
        ));
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
    /// Use the dense `FaerComplexEigensolver` oracle (full spectrum, about
    /// 10 s per 3300-DOF solve) instead of the default sparse shift-invert
    /// Lanczos.
    #[arg(long)]
    dense: bool,

    /// Run the σ₀ continuation sweep of the TE₁ triplets (issue #1026)
    /// instead of the three-point benchmark, writing
    /// `benchmarks/mie_sphere/open_sigma_sweep.toml`.
    #[arg(long)]
    sigma_sweep: bool,

    /// First σ₀ of the continuation sweep.
    #[arg(long, default_value_t = 5.0)]
    sigma_start: f64,

    /// Last σ₀ of the continuation sweep (inclusive).
    #[arg(long, default_value_t = 25.0)]
    sigma_end: f64,

    /// σ₀ step of the continuation sweep.
    #[arg(long, default_value_t = 0.5)]
    sigma_step: f64,

    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        type B = TestBackend;
        let device = <B as BackendTypes>::Device::default();

        let use_dense = self.dense;
        let f = read_sphere_fixture()?;
        eprintln!(
            "sphere fixture: {} nodes, {} tets",
            f.mesh.n_nodes(),
            f.mesh.n_tets()
        );

        let catalog = open_space_wgm_roots_n15();
        // Primary acceptance target TE₁,₁ (the b₁ pole, Q ≈ 1.95) first,
        // then best-effort TM₁,₁ (the a₁ pole, Q ≈ 0.72). Before issue
        // #999 the catalog called these TM₁,₁ and TE₁,₁ respectively.
        let targets: Vec<&MieRootComplex> = [
            (MiePolarisation::TE, 1usize, 1usize),
            (MiePolarisation::TM, 1, 1),
        ]
        .iter()
        .map(|&(pol, l, n)| {
            catalog
                .iter()
                .find(|r| r.pol == pol && r.l == l && r.n == n)
                .expect("target root in catalog")
        })
        .collect();

        if self.sigma_sweep {
            sweep::run_sweep::<B>(
                &device,
                &f,
                targets[0],
                &sweep::SweepGrid {
                    start: self.sigma_start,
                    end: self.sigma_end,
                    step: self.sigma_step,
                },
            );
            return Ok(());
        }

        let mut rows = Vec::new();
        for &sigma_0 in SIGMA_VALUES {
            for root in &targets {
                let omega0 = root.re_k;
                eprintln!(
                    "=== σ₀ = {sigma_0}, target {}_{},{} (analytic k = {:.4} {:+.4}j, Q = {:.3}) ===",
                    root.pol.as_str(),
                    root.l,
                    root.n,
                    root.re_k,
                    root.im_k,
                    root.q()
                );
                let modes = solve_frozen_omega::<B>(&device, &f, sigma_0, omega0, use_dense);
                eprintln!("  {} oscillatory physical modes", modes.len());
                let Some(matched) = match_root(&modes, root) else {
                    eprintln!("  no physical mode returned — skipping row");
                    continue;
                };
                let family = family_of(root);
                match matched.multiplet.as_ref().and_then(|m| m.first()) {
                    Some(m) => eprintln!(
                        "  identified {} multiplet, nearest member: k = {:.4} {:+.4}j, Q = {:.3} \
                         (rel err Re(k) = {:.2}%, Q ratio = {:.3}){}",
                        family.label(),
                        m.k.re,
                        m.k.im,
                        m.q(),
                        (m.k.re - root.re_k).abs() / root.re_k * 100.0,
                        m.q() / root.q(),
                        if matched.nearest_is_member {
                            String::new()
                        } else {
                            format!(
                                "; the nearest mode is NOT a member (it is {})",
                                family_label(&matched.nearest)
                            )
                        }
                    ),
                    None => eprintln!(
                        "  NO complete {} multiplet identified; nearest mode is {} at \
                         k = {:.4} {:+.4}j",
                        family.label(),
                        family_label(&matched.nearest),
                        matched.nearest.k.re,
                        matched.nearest.k.im
                    ),
                }

                // One Picard refresh for the primary TE target: re-freeze Λ
                // at the identified mode's Re(k), re-solve and re-identify.
                let picard = match (
                    root.pol == MiePolarisation::TE,
                    matched.multiplet.as_ref().and_then(|m| m.first()),
                ) {
                    (true, Some(m0)) => {
                        let omega1 = m0.k.re;
                        eprintln!("  Picard refresh at ω₁ = {omega1:.4} …");
                        let modes1 =
                            solve_frozen_omega::<B>(&device, &f, sigma_0, omega1, use_dense);
                        let refreshed = match_root(&modes1, root)
                            .and_then(|m| m.multiplet)
                            .and_then(|m| m.first().copied());
                        match refreshed {
                            Some(m1) => {
                                eprintln!(
                                    "  Picard: k = {:.4} {:+.4}j, Q = {:.3} \
                                     (ΔRe(k) = {:+.2e}, ΔQ = {:+.3})",
                                    m1.k.re,
                                    m1.k.im,
                                    m1.q(),
                                    m1.k.re - m0.k.re,
                                    m1.q() - m0.q()
                                );
                                Some((omega1, m1.k.re, m1.k.im, m1.q()))
                            }
                            None => {
                                eprintln!("  Picard: no complete multiplet after the refresh");
                                None
                            }
                        }
                    }
                    _ => None,
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
                    matched,
                    picard,
                });
            }
        }

        write_results(&rows);

        eprintln!("\nSummary (identified multiplets vs. open-space analytic roots):");
        for r in &rows {
            let head = format!(
                "  σ₀ = {:>4}: {}_{},{}",
                r.sigma_0,
                r.pol.as_str(),
                r.l,
                r.n
            );
            match r.reported() {
                Some(m) => eprintln!(
                    "{head}  Re(k) {:.4} vs {:.4} ({:.1}% err), Q {:.3} vs {:.3} (ratio {:.3}){}",
                    m.k.re,
                    r.analytic_re_k,
                    (m.k.re - r.analytic_re_k).abs() / r.analytic_re_k * 100.0,
                    m.q(),
                    r.analytic_q,
                    m.q() / r.analytic_q,
                    if r.matched.nearest_is_member {
                        String::new()
                    } else {
                        format!(
                            " [nearest mode is {}, not a member]",
                            family_label(&r.matched.nearest)
                        )
                    }
                ),
                None => eprintln!(
                    "{head}  not identified (nearest mode is {})",
                    family_label(&r.matched.nearest)
                ),
            }
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
