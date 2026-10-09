//! **Lossy / open PEC-walled cavity eigenmodes on any tagged tet mesh**
//! (issue #706, Epic #702 Phase 4): complex materials, optional matched
//! box-UPML absorbing shells, PEC surfaces in — complex resonant
//! wavenumbers (hence frequency **and** `Q`) out.
//!
//! This is the complex analogue of [`crate::eigen::pec_cavity`] (the
//! lossless path behind `geode eigen`) and mirrors its shape: a
//! `Settings` / `Mode` / `Modes` / `Error` family, a sparse
//! interior-reduced pencil assembly, a gradient-nullspace filter and a
//! per-mode residual acceptance gate.
//!
//! # The pencil
//!
//! First-order Nédélec (Whitney) edge elements:
//!
//! ```text
//! K x = λ M x,   K_ij = ∫ ∇×N_i · ν ∇×N_j dV,   M_ij = ∫ N_i · ε N_j dV,
//! λ = k₀²  (k₀ = ω/c in rad per mesh length unit, now complex)
//! ```
//!
//! restricted to the interior (non-PEC) edges. Two material models
//! ([`LossyCavityMaterials`]):
//!
//! - **Isotropic complex `ε_r`** per tet (`ν = 1`) — assembled with
//!   [`assemble_global_nedelec_with_complex_epsilon_sparse`], the same
//!   pattern-aligned kernel the lossless path uses with `Im(ε_r) = 0`.
//!   Passive media only: `Im(ε_r) ≤ 0` under the repo-wide `exp(+jωt)`
//!   convention (`ε_r = ε′ − jε″`); gain (`Im > 0`) is rejected.
//! - **Full complex tensors** `(ε, ν)` per tet — assembled with
//!   [`assemble_global_nedelec_with_full_tensors_sparse`], the matched-UPML
//!   assembler the driven solver already uses. The caller supplies the
//!   tensors; for a box UPML they are `ε = ε_r·Λ(k₀)`, `ν = Λ(k₀)⁻¹`
//!   ([`crate::mesh::patch::box_upml_tensors`]) evaluated **once** at a fixed
//!   reference `k₀` (the `geode eigen` CLI uses the shift, `k₀ = √σ`).
//!
//! Both pencils are complex **symmetric** (not Hermitian) and are solved
//! with the pure-Rust sparse complex shift-invert Lanczos (bilinear
//! `M`-inner product, direct sparse-LU inner solve), eigenvectors
//! included, through its residual-checked, filtered entry point
//! ([`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked_filtered`],
//! issue #834). The bilinear Lanczos has no interlacing guarantee, so an
//! unconverged Ritz value can sit nearer `σ` than converged modes; the
//! checked solve never returns one. It withholds it and extends the same
//! Krylov run (to at most `2 · max_iters` steps). The result is the
//! `n_modes` **converged modes nearest `σ`**, or an error:
//!
//! - a withheld pair that is not *localized* (`ρ · max(|λ|, σ) > |λ − σ|`,
//!   for example the `ρ ≈ 11` value of PR #833) locates no eigenvalue and
//!   is skipped;
//! - a localized withheld pair is a genuine eigenvalue that did not converge
//!   by the cap. If it lies nearer `σ` than the farthest returned mode, the
//!   solve fails with [`LossyCavityError::NotConverged`] rather than let a
//!   farther mode fill its slot (PR #847). This applies only when the
//!   pair's whole uncertainty disc `|μ − λ| ≤ ρ · max(|λ|, σ)` lies in the
//!   eligible region (clear of the null disc and of `Re μ ≤ 0`). A disc
//!   that reaches a screened region may be locating a screened eigenvalue.
//!   An example is a ghost of the gradient nullspace with `ρ ≈ 0.1`
//!   (issue #968). Such a pair is skipped;
//! - fewer than `n_modes` converged physical pairs is also `NotConverged`
//!   (or [`LossyCavityError::TooFewModes`]).
//!
//! When the first `max_iters` pass already converged them, the result is
//! bit-identical to the unchecked solve.
//!
//! # Fixed-frequency UPML (a documented approximation)
//!
//! A UPML stretch `s = 1 − jσ₀(d/w)²/k₀` depends on the frequency, so the
//! exact open-cavity problem is a **nonlinear** eigenproblem `A(λ)x = 0`.
//! This module solves the **linear** pencil obtained by freezing the
//! stretch at one reference `k₀`. Modes whose `Re(k₀)` lies near that
//! reference see the absorber as designed; modes far from it see a
//! mistuned one (the stretch still attenuates, but its profile was set
//! for the reference). Self-consistent iteration of the reference toward
//! the converged `Re(k₀)` (the fixed-point architecture of
//! [`crate::eigen::self_consistent`], today Silver-Müller only) is a
//! natural follow-on and is **not** done here.
//!
//! Frequency-dependent surface terms (Leontovich walls,
//! `Z_s ∝ √(jωμ₀/σ)`; the Silver-Müller term, linear in `k₀`) make the
//! operator depend nonlinearly on its own eigenvalue in the same way and
//! are out of scope for this linear solver.
//!
//! # Sign convention and `Q`
//!
//! `exp(+jωt)` time dependence; `k₀ = √λ` on the principal branch
//! (`Re k₀ ≥ 0`, [`principal_k0`]); `Im(k₀) > 0` is a mode that **decays**
//! in time (lossy or radiating); `Q = Re(k₀) / (2 Im(k₀))`. This matches
//! [`crate::eigen::self_consistent`] and `geode_util::eigen::q_factor`
//! (which the CLI uses to report `Q` — this module deliberately returns
//! the complex `k₀`, not `Q`, to avoid another copy of the formula).
//!
//! For a PEC cavity **uniformly** filled with one lossy dielectric
//! (`ε_r = ε′ − jε″`, `tan δ = ε″/ε′`), `M_ε = ε_r M₁`, so the lossy
//! eigenvectors equal the lossless ones and `λ = μ₀/ε_r` exactly, giving
//! `Im(λ)/Re(λ) = tan δ` and `Q = ½·cot(δ/2)` at any loss level (the
//! textbook `Q = 1/tan δ` is only its small-δ leading order). That exact
//! identity is the golden regression target (see the unit tests and
//! `crates/geode-cli/tests/sphere_lossy_pec_golden.rs`).
//!
//! # Shift placement, the gradient nullspace and overdamped modes
//!
//! As in [`crate::eigen::pec_cavity`]: `K` has the discrete-gradient
//! nullspace at `λ = 0` (a UPML `ν` does not change that — `∇×∇φ = 0`),
//! so `σ` must be strictly positive (rejected up front here; a collapse
//! onto a singular shift is also caught post-solve by
//! [`crate::eigen::shift_guard`] and surfaces as
//! [`LossyCavityError::Eigen`]`(`[`EigenError::DegenerateShift`]`)`).
//!
//! Ritz values with `|λ| ≤ null_tol_rel·σ` are classified as gradient
//! nullspace and dropped (default `1e-3`, same as the lossless path: the
//! null cluster sits at `|λ| ~ 1e-14·scale`, far below any physical
//! mode). Ritz values with `Re(λ) ≤ 0` (`|Im k₀| ≥ Re k₀`, `Q ≤ ½`) are
//! dropped as **overdamped** — not resonances. No exact eigenvalue of a
//! closed passive cavity lands there (for `K x = λ M x` with real PSD `K`
//! and `M = Σ ε_r,i M_i`, `Im ε_r,i ≤ 0`, the Rayleigh quotient puts
//! `arg λ ∈ [0, δ_max]`), only the occasional unconverged Ritz value from
//! the far end of the Lanczos basis; an open (UPML) pencil has a cloud of
//! absorber-dominated
//! quasi-modes around the origin, many with `Re(λ) < 0` (measured on the
//! `patch_2g4_smoke` box-UPML fixture: dozens between `−1e-3` and `0`,
//! several of them poorly converged). The remaining `n_modes` closest to
//! `σ` (by `|λ − σ|` in the complex plane) are returned, ascending in
//! resonant frequency `Re(k₀)` (not `Re(λ) = Re(k₀)² − Im(k₀)²`, which
//! would reorder strongly damped modes).
//!
//! UPML quasi-modes with `Re(λ) > 0` but low `Q` (field concentrated in
//! the absorber) are genuine eigenpairs of the discrete pencil and are
//! **not** filtered: place `σ` close to the resonance of interest (the
//! closest-to-`σ` selection then prefers it) and read `Q` to tell them
//! apart. No energy-fraction PML-mode classifier is applied.

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_complex_epsilon_sparse,
    assemble_global_nedelec_with_full_tensors_sparse,
};
use crate::assembly::p1::upload_mesh;
use crate::eigen::complex::{CheckedComplexEigenpairs, SparseComplexShiftInvertLanczos};
use crate::eigen::dense::EigenError;
use crate::eigen::hcurl_null::{
    GRADIENT_FRACTION_CUT, GradientNullCount, GradientNullSpace, LoopHarmonics,
};
use crate::eigen::lanczos::ConvergenceCheck;
use crate::elements::ElementOrder;
use crate::mesh::{TaggedTetMesh, TetMesh, pec_interior_mask_from_triangles};

/// Errors from the lossy / open cavity eigensolve.
#[derive(Debug, thiserror::Error)]
pub enum LossyCavityError {
    /// An input slice or setting is malformed (wrong length, non-finite,
    /// gain medium, non-positive shift, zero modes requested, …).
    #[error("invalid lossy cavity eigen input: {0}")]
    InvalidInput(String),
    /// A named physical group does not exist in the mesh in the required
    /// dimension (`3` = volume region, `2` = surface).
    #[error("no dimension-{dim} physical group named `{name}` in the mesh")]
    UnknownGroup {
        /// Required dimension.
        dim: i32,
        /// The unresolved name.
        name: String,
    },
    /// Every edge is on a PEC wall: the interior pencil is empty.
    #[error("every edge is PEC — the interior pencil is empty")]
    EmptyInterior,
    /// The sparse eigensolve failed: LU factorization of `K − σM`, the
    /// complex Lanczos iteration, or the post-solve degenerate-shift
    /// guard ([`EigenError::DegenerateShift`], issue #696).
    #[error("eigensolve failed: {0}")]
    Eigen(#[from] EigenError),
    /// Fewer physical modes than requested were resolved near `σ`.
    #[error(
        "only {found} of {requested} requested physical modes resolved near sigma = {sigma} \
         (raise max_iters, or move sigma closer to the band of interest)"
    )]
    TooFewModes {
        /// Modes requested.
        requested: usize,
        /// Physical modes actually resolved.
        found: usize,
        /// The shift.
        sigma: f64,
    },
    /// The order-generic pencil assembly (issue #871) rejected its input
    /// (a space or wall that does not belong to the mesh, a singular wall
    /// impedance at the reference frequency, …).
    #[error("lossy cavity pencil assembly failed: {0}")]
    Assembly(#[from] crate::driven::solve::DrivenError),
    /// Null tripwire (issue #871): a returned mode lies in the exact
    /// gradient null space (gradient fraction `≥ ½`,
    /// [`crate::eigen::hcurl_null`]). It is a curl-free Ritz pair that
    /// escaped the magnitude filter, not a resonance; nothing is returned.
    #[error(
        "returned mode {index} (λ = {lambda_re} {lambda_im:+}j) is a gradient (gradient \
         fraction {fraction:.3}); raise null_tol_rel or move sigma"
    )]
    GradientModeReturned {
        /// Index of the mode in the ascending-`Re(k₀)` order.
        index: usize,
        /// Real part of its eigenvalue.
        lambda_re: f64,
        /// Imaginary part of its eigenvalue.
        lambda_im: f64,
        /// Its gradient fraction.
        fraction: f64,
    },
    /// A mode among the `n_modes` nearest `σ` did not converge: its
    /// relative eigen-residual exceeds [`LossyCavitySettings::residual_tol`]
    /// even after the Krylov extension. Reports the **worst** offending
    /// mode, or, when converged modes farther from `σ` would otherwise fill
    /// its slot (a localized withheld pair nearer `σ` than the farthest
    /// returned mode, PR #847), that withheld pair.
    #[error(
        "eigensolve did not converge: mode {index} (λ = {lambda_re} {lambda_im:+}j) has relative \
         residual {residual_rel:e} > bound {bound:e} (raise max_iters, or move sigma closer to \
         the band of interest)"
    )]
    NotConverged {
        /// Index (in the ascending-`Re(k₀)` returned order) of the worst mode.
        index: usize,
        /// Real part of its Ritz value.
        lambda_re: f64,
        /// Imaginary part of its Ritz value.
        lambda_im: f64,
        /// Its relative residual (may be non-finite).
        residual_rel: f64,
        /// The acceptance bound it exceeded.
        bound: f64,
    },
}

/// Settings for [`solve_lossy_cavity_modes`] (same meaning and defaults
/// as [`crate::eigen::pec_cavity::PecCavitySettings`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossyCavitySettings {
    /// Real shift `σ = k₀,target²` in `(rad / mesh length unit)²`, `> 0`.
    pub sigma: f64,
    /// Number of physical modes to return (`≥ 1`).
    pub n_modes: usize,
    /// Lanczos basis size (outer iterations).
    pub max_iters: usize,
    /// Lanczos relative convergence tolerance.
    pub tol: f64,
    /// Gradient-nullspace filter: Ritz values `|λ| ≤ null_tol_rel · σ`
    /// are classified as the curl-free near-kernel and dropped.
    pub null_tol_rel: f64,
    /// Acceptance bound on each returned mode's relative eigen-residual
    /// [`LossyCavityMode::residual_rel`] (default `1e-6`).
    pub residual_tol: f64,
}

impl LossyCavitySettings {
    /// Default Lanczos basis size.
    pub const DEFAULT_MAX_ITERS: usize = 160;
    /// Default Lanczos tolerance.
    pub const DEFAULT_TOL: f64 = 1e-9;
    /// Default gradient-nullspace filter (relative to `σ`). Same value as
    /// the lossless path (not `examples/mie_sphere`'s `0.5`, which that
    /// example needs for its spherical-PML pencil): on the bundled
    /// `sphere.msh` with a uniform lossy fill (`ε_r = 2.25 − 0.0225j`,
    /// `σ = 1`) the filter drops 12 gradient Ritz values and the lowest
    /// physical mode sits at `|λ| ≈ 0.84`, so `1e-3·σ` separates them
    /// with a wide margin.
    pub const DEFAULT_NULL_TOL_REL: f64 = 1e-3;
    /// Default per-mode relative-residual acceptance bound.
    pub const DEFAULT_RESIDUAL_TOL: f64 = 1e-6;

    /// Settings with the defaults for everything but `sigma` / `n_modes`.
    pub fn new(sigma: f64, n_modes: usize) -> Self {
        Self {
            sigma,
            n_modes,
            max_iters: Self::DEFAULT_MAX_ITERS,
            tol: Self::DEFAULT_TOL,
            null_tol_rel: Self::DEFAULT_NULL_TOL_REL,
            residual_tol: Self::DEFAULT_RESIDUAL_TOL,
        }
    }
}

/// Per-tet material model of the lossy pencil.
#[derive(Debug, Clone, Copy)]
pub enum LossyCavityMaterials<'a> {
    /// Isotropic complex relative permittivity per tet (`ν = 1`):
    /// `Re > 0`, `Im ≤ 0` (passive, `exp(+jωt)`).
    Isotropic(&'a [c64]),
    /// Full complex constitutive tensors per tet: `eps` weights the mass,
    /// `nu` (inverse relative permeability) the curl-curl stiffness —
    /// e.g. matched box-UPML tensors frozen at one reference `k₀`.
    Tensor {
        /// Per-tet `ε` tensor.
        eps: &'a [[[c64; 3]; 3]],
        /// Per-tet `ν` tensor.
        nu: &'a [[[c64; 3]; 3]],
    },
}

/// One physical (complex) cavity mode.
#[derive(Debug, Clone)]
pub struct LossyCavityMode {
    /// Complex eigenvalue `λ = k₀²` in `(rad / mesh length unit)²`.
    pub lambda: c64,
    /// Complex resonant wavenumber `k₀ = √λ` on the principal branch
    /// (`Re k₀ ≥ 0`); `Im k₀ > 0` = decaying (lossy / radiating) mode.
    pub k0: c64,
    /// Relative eigen-residual `‖K x − λ M x‖₂ / (|λ| ‖M x‖₂)` (Hermitian
    /// 2-norms). Always `≤ settings.residual_tol` for a returned mode.
    pub residual_rel: f64,
    /// Bilinear-`M`-normalized (`xᵀ M x = 1`, no conjugation) complex
    /// eigenvector over the **interior** edges (the `true` entries of the
    /// PEC mask, in global edge order).
    pub vector: Vec<c64>,
}

/// Result of [`solve_lossy_cavity_modes`].
#[derive(Debug, Clone)]
pub struct LossyCavityModes {
    /// Physical modes, ascending in resonant frequency `Re(k₀)`.
    pub modes: Vec<LossyCavityMode>,
    /// Interior (non-PEC) edge-DOF count — the pencil dimension.
    pub n_interior: usize,
    /// Ritz values dropped as gradient nullspace (`|λ| ≤ null_tol_rel·σ`).
    pub n_null_filtered: usize,
    /// Non-null Ritz values dropped as **overdamped** (`Re(λ) ≤ 0`, i.e.
    /// `Q ≤ ½`): not resonances. On an open (UPML) pencil these are
    /// absorber-trapped / evanescent quasi-modes clustered around the
    /// origin; on a closed passive cavity only unconverged Ritz values.
    pub n_overdamped_filtered: usize,
    /// Unconverged Ritz pairs the residual-checked Lanczos withheld
    /// although they ranked among the `n_modes` physical pairs nearest `σ`
    /// ([`crate::eigen::complex::CheckedComplexEigenpairs::rejected`],
    /// issue #834). The complex bilinear Lanczos has no interlacing
    /// guarantee, so such a value can sit nearer `σ` than converged modes;
    /// it is never returned. On a successful solve none of them is a
    /// localized pair nearer `σ` than a returned mode (that is an error), so
    /// each is spurious or lies beyond the returned range. `0` when the
    /// first Krylov pass converged.
    pub n_withheld: usize,
    /// Lanczos steps run: `max_iters` (plus 2) unless an unconverged pair
    /// near `σ` made the solve extend the Krylov run (at most
    /// `2 · max_iters` steps).
    pub lanczos_steps: usize,
}

/// Interior-reduced complex sparse pencil `(K, M)`.
pub type LossyPencil = (SparseColMat<usize, c64>, SparseColMat<usize, c64>);

/// Principal-branch complex wavenumber `k = √λ` (`Re k ≥ 0`, and
/// `sign(Im k) = sign(Im λ)`), the convention of
/// `examples/mie_sphere::fem_complex_k` and `geode_util::eigen::k_from_lambda`.
///
/// Delegates to the cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`]: `Im k₀` (and so the
/// reported `Q`) is accurate to a few ulps at any `Q`, where the old
/// `√(½(|λ| − Re λ))` form lost `≈ log₁₀(2Q²)` digits and returned
/// `Im k₀ = 0` (`Q = ∞`) from `Q ≈ 6.7e7` (issue #830).
pub fn principal_k0(lambda: c64) -> c64 {
    crate::eigen::wavenumber::principal_sqrt(lambda)
}

fn validate_materials(
    n_tets: usize,
    mat: &LossyCavityMaterials<'_>,
) -> Result<(), LossyCavityError> {
    let invalid = |m: String| LossyCavityError::InvalidInput(m);
    let finite = |z: &c64| z.re.is_finite() && z.im.is_finite();
    match mat {
        LossyCavityMaterials::Isotropic(eps) => {
            if eps.len() != n_tets {
                return Err(invalid(format!(
                    "eps_r has {} entries, mesh has {n_tets} tets",
                    eps.len()
                )));
            }
            if let Some(bad) = eps.iter().find(|e| !(finite(e) && e.re > 0.0)) {
                return Err(invalid(format!(
                    "eps_r must be finite with Re > 0 (got {} {:+}j)",
                    bad.re, bad.im
                )));
            }
            if let Some(bad) = eps.iter().find(|e| e.im > 0.0) {
                return Err(invalid(format!(
                    "eps_r has Im > 0 (gain: {} {:+}j); passive media need Im(eps_r) <= 0 \
                     under the exp(+jwt) convention",
                    bad.re, bad.im
                )));
            }
        }
        LossyCavityMaterials::Tensor { eps, nu } => {
            for (what, t) in [("eps", eps), ("nu", nu)] {
                if t.len() != n_tets {
                    return Err(invalid(format!(
                        "{what} tensor has {} entries, mesh has {n_tets} tets",
                        t.len()
                    )));
                }
                if t.iter().flatten().flatten().any(|z| !finite(z)) {
                    return Err(invalid(format!("{what} tensor has a non-finite entry")));
                }
            }
        }
    }
    Ok(())
}

/// Assemble the interior-reduced complex pencil `(K, M)` sparse.
///
/// `pec_interior_mask` is the per-edge mask over `mesh.edges()` (`true` =
/// kept interior DOF). Interior DOFs are renumbered contiguously in global
/// edge order.
///
/// # Errors
///
/// [`LossyCavityError::InvalidInput`] on length / value mismatches (incl.
/// gain media), [`LossyCavityError::EmptyInterior`] when no edge survives
/// the mask.
pub fn assemble_lossy_pencil<B: Backend>(
    mesh: &TetMesh,
    materials: &LossyCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    device: &B::Device,
) -> Result<LossyPencil, LossyCavityError> {
    validate_materials(mesh.n_tets(), materials)?;
    let tet_edges = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    if pec_interior_mask.len() != n_edges {
        return Err(LossyCavityError::InvalidInput(format!(
            "PEC mask has {} entries, mesh has {n_edges} edges",
            pec_interior_mask.len()
        )));
    }
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let mut remap = vec![usize::MAX; n_edges];
    let mut n_interior = 0usize;
    for (i, &keep) in pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n_interior;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(LossyCavityError::EmptyInterior);
    }

    let scatter = NedelecScatterMap::new(&tet_idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);
    let vals =
        |t: burn::tensor::Tensor<B, 1>| -> Vec<f64> { t.into_data().iter::<f64>().collect() };
    // (Re K, Im K, Re M, Im M) values in pattern order.
    let (k_re, k_im, m_re, m_im) = match materials {
        LossyCavityMaterials::Isotropic(eps) => {
            let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
                nodes_t, tets_t, &tet_sign, &scatter, eps,
            );
            let k_re = vals(sys.k_vals);
            let k_im = vec![0.0; k_re.len()];
            (k_re, k_im, vals(sys.m_re_vals), vals(sys.m_im_vals))
        }
        LossyCavityMaterials::Tensor { eps, nu } => {
            let sys = assemble_global_nedelec_with_full_tensors_sparse(
                nodes_t, tets_t, &tet_sign, &scatter, eps, nu,
            );
            (
                vals(sys.k_re_vals),
                vals(sys.k_im_vals),
                vals(sys.m_re_vals),
                vals(sys.m_im_vals),
            )
        }
    };

    let pattern = scatter.pattern();
    let mut k_tr = Vec::with_capacity(pattern.nnz());
    let mut m_tr = Vec::with_capacity(pattern.nnz());
    for (idx, (&r, &c)) in pattern.rows.iter().zip(&pattern.cols).enumerate() {
        let (rr, cc) = (remap[r as usize], remap[c as usize]);
        if rr == usize::MAX || cc == usize::MAX {
            continue;
        }
        k_tr.push(Triplet::new(rr, cc, c64::new(k_re[idx], k_im[idx])));
        m_tr.push(Triplet::new(rr, cc, c64::new(m_re[idx], m_im[idx])));
    }
    let build = |tr: &[Triplet<usize, usize, c64>], what: &str| {
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, tr)
            .map_err(|e| LossyCavityError::Eigen(EigenError::FaerGevd(format!("{what}: {e:?}"))))
    };
    Ok((
        build(&k_tr, "lossy cavity K")?,
        build(&m_tr, "lossy cavity M")?,
    ))
}

/// `y = A x` for a column-major complex sparse `A`.
fn spmv(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> Vec<c64> {
    let mut y = vec![c64::new(0.0, 0.0); a.nrows()];
    for (j, &xj) in x.iter().enumerate() {
        if xj.re == 0.0 && xj.im == 0.0 {
            continue;
        }
        for (i, v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            y[i] += *v * xj;
        }
    }
    y
}

/// Hermitian 2-norm.
fn norm2(v: &[c64]) -> f64 {
    v.iter()
        .map(|z| z.re * z.re + z.im * z.im)
        .sum::<f64>()
        .sqrt()
}

fn validate_settings(s: &LossyCavitySettings) -> Result<(), LossyCavityError> {
    let invalid = |m: String| LossyCavityError::InvalidInput(m);
    if !(s.sigma.is_finite() && s.sigma > 0.0) {
        return Err(invalid(format!(
            "sigma must be finite and > 0 (got {}); sigma = 0 makes K − σM singular on the \
             curl-curl gradient nullspace",
            s.sigma
        )));
    }
    if s.n_modes == 0 {
        return Err(invalid("n_modes must be ≥ 1".into()));
    }
    if s.max_iters == 0 {
        return Err(invalid("max_iters must be ≥ 1".into()));
    }
    for (what, v) in [("tol", s.tol), ("residual_tol", s.residual_tol)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(invalid(format!("{what} must be finite and > 0 (got {v})")));
        }
    }
    if !(s.null_tol_rel.is_finite() && s.null_tol_rel >= 0.0) {
        return Err(invalid(format!(
            "null_tol_rel must be finite and ≥ 0 (got {})",
            s.null_tol_rel
        )));
    }
    Ok(())
}

/// Solve the lossy / open PEC-cavity eigenproblem on `mesh`.
///
/// `materials`: per-tet material model ([`LossyCavityMaterials`]).
/// `pec_interior_mask`: per-edge mask over `mesh.edges()` (`true` = kept).
/// Returns the `settings.n_modes` physical modes closest to
/// `settings.sigma` in the complex plane, ascending in `Re(k₀)` (see the
/// module docs for the null / overdamped filters).
///
/// # Errors
///
/// [`LossyCavityError::InvalidInput`] for malformed inputs/settings,
/// [`LossyCavityError::Eigen`] if the factorization or Lanczos fails
/// (including [`EigenError::DegenerateShift`] from the post-solve
/// shift guard), [`LossyCavityError::TooFewModes`] if fewer than
/// `n_modes` physical modes were resolved, and
/// [`LossyCavityError::NotConverged`] if any selected mode's relative
/// residual exceeds [`LossyCavitySettings::residual_tol`], or if a
/// localized but unconverged Ritz pair (a genuine eigenvalue the capped
/// extension could not converge) lies nearer `σ` than the farthest mode
/// that would be returned: the result would have a hole.
pub fn solve_lossy_cavity_modes<B: Backend>(
    mesh: &TetMesh,
    materials: &LossyCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    settings: &LossyCavitySettings,
    device: &B::Device,
) -> Result<LossyCavityModes, LossyCavityError> {
    validate_settings(settings)?;
    let (k, m) = assemble_lossy_pencil::<B>(mesh, materials, pec_interior_mask, device)?;
    solve_lossy_pencil_modes(k.as_ref(), m.as_ref(), settings)
}

/// The eigensolve and mode selection of [`solve_lossy_cavity_modes`] on an
/// assembled interior pencil `(K, M)` (settings already validated). Split
/// out so the selection rules can be pinned on hand-built pencils.
fn solve_lossy_pencil_modes(
    k: SparseColMatRef<'_, usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    settings: &LossyCavitySettings,
) -> Result<LossyCavityModes, LossyCavityError> {
    let s = settings;
    let n_interior = k.nrows();

    // Screen every Ritz pair the basis yields *before* the closest-to-σ
    // truncation (see `pec_cavity`), and keep only converged pairs: the
    // complex bilinear Lanczos has no interlacing guarantee, so an
    // unconverged Ritz value can sit nearer σ than converged modes (issue
    // #834). When the first pass (exactly the historical `max_iters` run)
    // leaves one among the `n_modes` nearest σ, the same Krylov run is
    // extended, to at most `2 · max_iters` steps.
    let solver = SparseComplexShiftInvertLanczos {
        sigma: s.sigma,
        max_iters: s.max_iters,
        tol: s.tol,
    };
    let null_ceiling = s.null_tol_rel * s.sigma;
    let is_null = |l: c64| l.re.hypot(l.im) <= null_ceiling;
    // Overdamped filter: Re(λ) ≤ 0 ⇔ |Im k₀| ≥ Re k₀ ⇔ Q ≤ ½ — not a
    // resonance (UPML-trapped / evanescent quasi-modes of an open pencil).
    let is_overdamped = |l: c64| l.re <= 0.0;
    let eligible = |l: c64| !is_null(l) && !is_overdamped(l);
    let check = ConvergenceCheck {
        residual_tol: s.residual_tol,
        max_iters_cap: s.max_iters.saturating_mul(2),
        window: None,
    };
    let mut solve =
        solver.smallest_eigenpairs_checked_filtered(k, m, s.n_modes, check, &eligible)?;
    let n_null_filtered = solve.screened.iter().filter(|&&l| is_null(l)).count();
    let n_overdamped_filtered = solve.screened.len() - n_null_filtered;
    let n_withheld = solve.rejected.len();
    let lanczos_steps = solve.lanczos_steps;
    // This module's residual is relative to `|λ|` (`σ` only at `λ = 0`);
    // the checked solve's to `max(|λ|, σ)`, with the same numerator.
    let own_residual = |l: c64, checked: f64| {
        let mag = l.re.hypot(l.im);
        if mag != 0.0 {
            checked * mag.max(s.sigma) / mag
        } else {
            checked
        }
    };
    let candidates: Vec<LossyCavityMode> = std::mem::take(&mut solve.pairs)
        .into_iter()
        .map(|p| {
            let kx = spmv(k, &p.vector);
            let mx = spmv(m, &p.vector);
            let r: Vec<c64> = kx
                .iter()
                .zip(&mx)
                .map(|(a, b)| *a - p.lambda * *b)
                .collect();
            let mag = p.lambda.re.hypot(p.lambda.im);
            let scale = if mag != 0.0 { mag } else { s.sigma };
            LossyCavityMode {
                lambda: p.lambda,
                k0: principal_k0(p.lambda),
                residual_rel: norm2(&r) / (scale * norm2(&mx)),
                vector: p.vector,
            }
        })
        .collect();
    let dist = |l: c64| (l.re - s.sigma).hypot(l.im);
    let mut modes = candidates;
    modes.sort_by(|a, b| dist(a.lambda).total_cmp(&dist(b.lambda)));
    if modes.len() < s.n_modes {
        // Shortfall: the run ended (cap or breakdown) without `n_modes`
        // converged physical pairs. Report it as the unchecked solve did:
        // among the `n_modes` nearest σ of the converged and the withheld
        // pairs, the worst unconverged one is `NotConverged`.
        let mut slate: Vec<(c64, f64)> = modes
            .iter()
            .map(|m| (m.lambda, m.residual_rel))
            .chain(solve.rejected.iter().map(|&(l, r)| (l, own_residual(l, r))))
            .collect();
        slate.sort_by(|a, b| dist(a.0).total_cmp(&dist(b.0)));
        slate.truncate(s.n_modes);
        slate.sort_by(|a, b| principal_k0(a.0).re.total_cmp(&principal_k0(b.0).re));
        let worst = slate
            .iter()
            .enumerate()
            .filter(|(_, (_, r))| r.is_nan() || *r > s.residual_tol)
            .max_by(|(_, a), (_, b)| {
                let key = |x: f64| if x.is_nan() { f64::INFINITY } else { x };
                key(a.1).total_cmp(&key(b.1))
            });
        if let (true, Some((index, &(lambda, residual_rel)))) = (slate.len() == s.n_modes, worst) {
            return Err(LossyCavityError::NotConverged {
                index,
                lambda_re: lambda.re,
                lambda_im: lambda.im,
                residual_rel,
                bound: s.residual_tol,
            });
        }
        return Err(LossyCavityError::TooFewModes {
            requested: s.n_modes,
            found: modes.len(),
            sigma: s.sigma,
        });
    }
    modes.truncate(s.n_modes);
    // Hole check (PR #847 review): a withheld pair that is localized (a
    // genuine eigenvalue, still unconverged when the extension hit its cap)
    // and nearer σ than the farthest returned mode means farther converged
    // modes filled its slot. That is not "the n_modes converged modes
    // nearest σ"; fail as the unchecked solve did. A spurious Ritz value
    // (PR #833: ρ ≈ 11) is not localized and is still skipped, and so is a
    // localized pair whose uncertainty disc reaches the null or overdamped
    // region (issue #968): it may be locating a screened eigenvalue.
    let reach = modes.last().map_or(0.0, |m| dist(m.lambda));
    if let Some((lambda, checked)) = eligible_hole(&solve, s.sigma, reach, null_ceiling) {
        let residual_rel = own_residual(lambda, checked);
        let mut slate: Vec<(c64, f64)> = modes
            .iter()
            .map(|m| (m.lambda, m.residual_rel))
            .chain(std::iter::once((lambda, residual_rel)))
            .collect();
        slate.sort_by(|a, b| dist(a.0).total_cmp(&dist(b.0)));
        slate.truncate(s.n_modes);
        slate.sort_by(|a, b| principal_k0(a.0).re.total_cmp(&principal_k0(b.0).re));
        let index = slate
            .iter()
            .position(|&(l, _)| l == lambda)
            .unwrap_or_default();
        return Err(LossyCavityError::NotConverged {
            index,
            lambda_re: lambda.re,
            lambda_im: lambda.im,
            residual_rel,
            bound: s.residual_tol,
        });
    }
    // Ascending resonant frequency Re(k₀) — not Re(λ) = Re(k₀)² − Im(k₀)²,
    // which reorders strongly damped modes.
    modes.sort_by(|a, b| a.k0.re.total_cmp(&b.k0.re));

    // Residual acceptance gate (a NaN residual counts as the worst).
    let worst = modes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.residual_rel.is_nan() || m.residual_rel > s.residual_tol)
        .max_by(|(_, a), (_, b)| {
            let key = |x: f64| if x.is_nan() { f64::INFINITY } else { x };
            key(a.residual_rel).total_cmp(&key(b.residual_rel))
        });
    if let Some((index, m)) = worst {
        return Err(LossyCavityError::NotConverged {
            index,
            lambda_re: m.lambda.re,
            lambda_im: m.lambda.im,
            residual_rel: m.residual_rel,
            bound: s.residual_tol,
        });
    }
    Ok(LossyCavityModes {
        modes,
        n_interior,
        n_null_filtered,
        n_overdamped_filtered,
        n_withheld,
        lanczos_steps,
    })
}

/// The hole of [`solve_lossy_pencil_modes`]: the localized withheld pair
/// nearest `σ`, strictly nearer than `reach`, whose whole uncertainty disc
/// `|μ − λ| ≤ r` is **eligible**, that is clear of the null disc
/// `|μ| ≤ null_ceiling` and of the overdamped half-plane `Re μ ≤ 0`
/// ([`CheckedComplexEigenpairs::localized_hole_by_disc`], issue #968).
///
/// The disc is what the localization test asserts: some eigenvalue lies
/// in it. If the disc reaches a screened region, that eigenvalue may be a
/// screened one. The near-null Ritz values of a p=2 pencil are the
/// example. The large gradient nullspace contaminates them, and they come
/// out at `|λ|` above the null ceiling with `ρ ≈ 0.1–0.6`, which at
/// `σ ≈ 20` is a disc of radius `2–12` around a point within `0.3` of
/// `λ = 0`. They pass the point test (`Re λ > 0`, `|λ|` above the
/// ceiling) and the localization test (`|λ − σ| ≈ σ`), yet they locate the
/// gradient nullspace, not a physical mode. Testing the Ritz value alone
/// turned them into holes whenever `σ` was nearer `0` than the farthest
/// requested mode (`golden3a_uniform_lossy_fill_exact_q_at_p2`).
fn eligible_hole(
    solve: &CheckedComplexEigenpairs,
    sigma: f64,
    reach: f64,
    null_ceiling: f64,
) -> Option<(c64, f64)> {
    solve.localized_hole_by_disc(sigma, reach, |l, r| {
        l.norm() - r > null_ceiling && l.re - r > 0.0
    })
}

/// [`solve_lossy_cavity_modes`] on a Gmsh-tagged mesh with isotropic
/// complex materials, binding by physical-group **name**: `pec_groups`
/// are dimension-2 surfaces eliminated as PEC; `materials` maps
/// dimension-3 region names to a complex relative permittivity (unlisted
/// regions are vacuum, `ε_r = 1`).
///
/// Absorbing (UPML) shells are not bound here: their tensors depend on
/// the shell's box geometry and a reference frequency, which the caller
/// resolves (the `geode eigen` CLI does so from its spec's
/// `absorbing_regions`) and passes through
/// [`LossyCavityMaterials::Tensor`] to [`solve_lossy_cavity_modes`].
///
/// # Errors
///
/// [`LossyCavityError::UnknownGroup`] for a name missing from the mesh in
/// the required dimension, [`LossyCavityError::InvalidInput`] for a
/// region listed twice, plus everything [`solve_lossy_cavity_modes`]
/// returns.
pub fn solve_tagged_lossy_cavity_modes<B: Backend>(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, c64)],
    settings: &LossyCavitySettings,
    device: &B::Device,
) -> Result<LossyCavityModes, LossyCavityError> {
    let (eps_r, pec_tris) = tagged_eps_and_walls(tagged, pec_groups, materials)?;
    let lists: Vec<&[[u32; 3]]> = pec_tris.iter().map(Vec::as_slice).collect();
    let mask = pec_interior_mask_from_triangles(&tagged.mesh.edges(), &lists);
    solve_lossy_cavity_modes::<B>(
        &tagged.mesh,
        &LossyCavityMaterials::Isotropic(&eps_r),
        &mask,
        settings,
        device,
    )
}

/// A wall's triangles.
type TriangleList = Vec<[u32; 3]>;

/// Resolve a tagged lossy cavity's physical-group names: the per-tet
/// complex `ε_r` (unlisted regions vacuum) and the PEC wall triangle lists.
fn tagged_eps_and_walls(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, c64)],
) -> Result<(Vec<c64>, Vec<TriangleList>), LossyCavityError> {
    let tag_of = |dim: i32, name: &str| {
        tagged
            .physical_group_tag(dim, name)
            .ok_or_else(|| LossyCavityError::UnknownGroup {
                dim,
                name: name.to_string(),
            })
    };
    let mut eps_by_tag = std::collections::BTreeMap::new();
    for &(name, eps) in materials {
        if eps_by_tag.insert(tag_of(3, name)?, eps).is_some() {
            return Err(LossyCavityError::InvalidInput(format!(
                "material for `{name}` listed more than once"
            )));
        }
    }
    let eps_r: Vec<c64> = tagged
        .tet_physical_tags
        .iter()
        .map(|t| eps_by_tag.get(t).copied().unwrap_or(c64::new(1.0, 0.0)))
        .collect();
    let pec_tris = pec_groups
        .iter()
        .map(|name| Ok(tagged.triangles_with_tag(tag_of(2, name)?)))
        .collect::<Result<Vec<_>, LossyCavityError>>()?;
    Ok((eps_r, pec_tris))
}

// ---------------------------------------------------------------------------
// Order-generic entry points (issue #871, Epic #836 Phase 2)
// ---------------------------------------------------------------------------

/// Result of the order-generic lossy solves
/// ([`solve_lossy_cavity_modes_on_space`],
/// [`solve_tagged_lossy_cavity_modes_at_order`]).
#[derive(Debug, Clone)]
pub struct SpaceLossyCavityModes {
    /// Element order the pencil was actually assembled and solved at.
    pub order: ElementOrder,
    /// The modes, exactly as [`solve_lossy_cavity_modes`] reports them
    /// (eigenvectors over the kept DOFs of the space's mask, full-DOF
    /// order).
    pub modes: LossyCavityModes,
    /// The exact gradient null dimension of the pencil
    /// ([`crate::eigen::hcurl_null`]).
    pub gradient_null: GradientNullCount,
    /// The largest gradient fraction among the returned modes (`< ½` by
    /// the tripwire; round-off-small for converged modes). `None` at p=1,
    /// which is bit-identical to [`solve_lossy_cavity_modes`] and runs no
    /// classifier.
    pub max_gradient_fraction: Option<f64>,
    /// Per-mode relative offset `|Re k₀ − ω_ref| / ω_ref` of each returned
    /// mode from the frequency its impedance walls were frozen at
    /// ([`FrozenWalls`]), in the order of `modes.modes`. `None` when the
    /// pencil has no frequency-dependent wall (no walls, or London walls
    /// only, which are exact at every `ω`).
    pub wall_ref_offset: Option<Vec<f64>>,
    /// Advisory diagnostics (empty when nothing is flagged): one
    /// [`LossyCavityWarning::FrozenWallMismatch`] per mode whose
    /// `wall_ref_offset` exceeds [`FrozenWalls::warn_rel_offset`], then a
    /// [`LossyCavityWarning::HarmonicFieldsPresent`] when the mesh topology
    /// allows loop harmonics.
    pub warnings: Vec<LossyCavityWarning>,
}

/// Default [`FrozenWalls::warn_rel_offset`]: a returned mode more than 5 %
/// away from `ω_ref` is flagged. At that offset the wall loss (hence `Q`
/// and `Im k₀`) of the mode is off by roughly 2.5 % for a Leontovich /
/// rough-conductor wall and 5 % for a `Fixed` (Silver-Müller) wall; see
/// [`FrozenWalls`] for the scaling.
pub const DEFAULT_WALL_REF_WARN_OFFSET: f64 = 0.05;

/// An advisory diagnostic attached to [`SpaceLossyCavityModes::warnings`].
/// Warnings never change the returned modes.
#[derive(Debug, Clone, PartialEq)]
pub enum LossyCavityWarning {
    /// Mode `index` resonates at `Re k₀` far from the frequency `ω_ref`
    /// its frequency-dependent impedance walls were frozen at
    /// ([`FrozenWalls`]), so its wall loss — `Q`, `Im k₀`, and the wall
    /// reactance's pull on `Re k₀` — is only approximate. Re-solve with
    /// `omega_ref` near this mode's `Re k₀` for an accurate value.
    FrozenWallMismatch {
        /// Index of the mode in [`SpaceLossyCavityModes::modes`].
        index: usize,
        /// The mode's `Re k₀` (natural units, `= ω`).
        k0_re: f64,
        /// The reference frequency the walls were frozen at.
        omega_ref: f64,
        /// `|Re k₀ − ω_ref| / ω_ref`.
        rel_offset: f64,
        /// The threshold it exceeded ([`FrozenWalls::warn_rel_offset`]).
        threshold: f64,
    },
    /// The mesh is multiply connected (`b₁ > 0`) and its walls do not close
    /// its whole boundary, so the curl-curl operator can have **loop
    /// harmonics**: curl-free fields circulating through a handle that are
    /// neither gradients nor resonances
    /// ([`crate::eigen::hcurl_null`], "Loop harmonics"). They are outside
    /// [`SpaceLossyCavityModes::gradient_null`]`.dim` and the gradient
    /// classifier; `λ ≈ 0` harmonic pairs are removed by the magnitude
    /// filter only.
    HarmonicFieldsPresent {
        /// First Betti number `b₁` of the mesh (its handle count).
        b1: usize,
        /// The number of loop harmonics when `exact`, otherwise an upper
        /// bound on it (`≤ b1`).
        loop_harmonics: usize,
        /// Whether `loop_harmonics` is the exact count (no wall on the
        /// multiply-connected part) or only a bound.
        exact: bool,
    },
}

impl std::fmt::Display for LossyCavityWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrozenWallMismatch {
                index,
                k0_re,
                omega_ref,
                rel_offset,
                threshold,
            } => write!(
                f,
                "mode {index} (Re k0 = {k0_re:.6}) is {:.1} % from the wall reference frequency \
                 omega_ref = {omega_ref:.6} (threshold {:.1} %): its wall-loss Q and Re k0 are \
                 approximate; re-solve with omega_ref near {k0_re:.6}",
                100.0 * rel_offset,
                100.0 * threshold
            ),
            Self::HarmonicFieldsPresent {
                b1,
                loop_harmonics,
                exact,
            } => LoopHarmonics {
                b1: *b1,
                count: *loop_harmonics,
                exact: *exact,
            }
            .fmt(f),
        }
    }
}

/// Impedance walls of a lossy cavity pencil, **frozen at one reference
/// frequency** (issue #871).
///
/// A wall adds `(iω/Z_s(ω))·S_Γ` to the stiffness. That coefficient depends
/// on the eigenvalue itself (`√ω` for a good conductor, `ω` for the
/// Silver-Müller `Z_s = η₀`), so the exact problem is nonlinear in `λ`.
/// The linear pencil here evaluates it once at `omega_ref` (natural units,
/// `ω = k₀`), the same fixed-frequency linearization the box UPML uses:
/// modes with `Re k₀ ≈ omega_ref` see the wall as specified, and the loss
/// of a high-`Q` mode is first-order accurate in `|Re k₀ − omega_ref|`.
/// The London wall's coefficient `1/λ_L` is frequency-independent, so for
/// it the pencil is exact.
///
/// **Error scaling away from `ω_ref`.** With `δ = |Re k₀ − ω_ref|/ω_ref`,
/// the wall coefficient a mode sees is wrong by a relative
/// `|(ω/ω_ref)^α − 1| ≈ α·δ`, and its wall loss (`1/Q_wall`, `Im k₀`) and
/// wall-induced frequency pull are wrong by the same relative amount, to
/// first order. (For a good conductor the frozen wall stiffness is large,
/// the effective `Z_s` it imposes is `iRe k₀/coefficient`, and
/// `Q_frozen/Q = √(ω_ref/Re k₀)`: freezing below a mode under-estimates its
/// `Q`. Measured in the `eigen_p2_tagged` golden 3c to 0.03 %.)
///
/// | wall model | `iω/Z_s ∝` | `α` | wall-loss error |
/// |---|---|---|---|
/// | `GoodConductor` (Leontovich) | `√ω` | ½ | `≈ ½ δ` |
/// | `RoughConductor` | `K(ω)√ω` | `≥ ½` | `≳ ½ δ` (plus the `K(ω)` slope) |
/// | `Fixed` (incl. Silver-Müller `η₀`) | `ω` | 1 | `≈ δ` |
/// | `London` | `ω⁰` (`1/λ_L`) | 0 | exact |
///
/// A multi-mode solve over a band therefore gets an accurate `Q` only for
/// the modes near `ω_ref`. The solver reports every mode's `δ`
/// ([`SpaceLossyCavityModes::wall_ref_offset`]) and attaches a
/// [`LossyCavityWarning::FrozenWallMismatch`] to each mode with
/// `δ > warn_rel_offset` (default [`DEFAULT_WALL_REF_WARN_OFFSET`] = 5 %,
/// i.e. a wall-loss error of about 2.5 % Leontovich / 5 % Silver-Müller).
/// London-only walls are never flagged. To get every mode's `Q`, re-solve
/// once per mode with `omega_ref` set to that mode's `Re k₀` (one or two
/// fixed-point passes converge the first-order error away).
#[derive(Debug, Clone, Copy)]
pub struct FrozenWalls<'a> {
    /// The walls (triangles + `Z_s(ω)` model).
    pub walls: &'a [crate::driven::solve::SurfaceImpedanceBc<'a>],
    /// Reference angular frequency `ω_ref` (natural units, `> 0`).
    pub omega_ref: f64,
    /// Relative offset `|Re k₀ − ω_ref|/ω_ref` above which a returned mode
    /// gets a [`LossyCavityWarning::FrozenWallMismatch`] (advisory only;
    /// the modes are returned either way). Must be `≥ 0` (`+∞` disables
    /// the warning). [`FrozenWalls::new`] sets
    /// [`DEFAULT_WALL_REF_WARN_OFFSET`].
    pub warn_rel_offset: f64,
}

impl<'a> FrozenWalls<'a> {
    /// No walls.
    pub const NONE: FrozenWalls<'static> = FrozenWalls {
        walls: &[],
        omega_ref: 1.0,
        warn_rel_offset: DEFAULT_WALL_REF_WARN_OFFSET,
    };

    /// `walls` frozen at `omega_ref`, with the default mismatch warning
    /// threshold [`DEFAULT_WALL_REF_WARN_OFFSET`].
    #[must_use]
    pub const fn new(
        walls: &'a [crate::driven::solve::SurfaceImpedanceBc<'a>],
        omega_ref: f64,
    ) -> Self {
        Self {
            walls,
            omega_ref,
            warn_rel_offset: DEFAULT_WALL_REF_WARN_OFFSET,
        }
    }

    /// Whether any wall's weak coefficient depends on `ω` (everything but
    /// London), i.e. whether freezing it at `omega_ref` is approximate.
    #[must_use]
    pub fn is_frequency_dependent(&self) -> bool {
        use crate::driven::solve::SurfaceImpedanceModel;
        self.walls
            .iter()
            .any(|w| !matches!(w.model, SurfaceImpedanceModel::London { .. }))
    }
}

/// [`assemble_lossy_pencil`] on an order-pluggable [`HcurlSpace`] (issue
/// #871, Epic #836 Phase 2), optionally with impedance walls frozen at a
/// reference frequency ([`FrozenWalls`]: Leontovich good / rough conductor,
/// London, `Fixed` incl. Silver-Müller).
///
/// `pec_interior_mask` is over `space.n_dofs()` (build it with
/// [`HcurlSpace::pec_interior_mask`]).
///
/// - **p=1, no walls:** [`assemble_lossy_pencil`] verbatim (bit-identical).
///   Walls add their Whitney surface masses
///   ([`crate::assembly::surface::assemble_surface_mass_triplets`]) times
///   `iω_ref/Z_s(ω_ref)` on the kept edges.
/// - **p=2:** the host-side p=2 assembly of
///   [`crate::driven::solve::DrivenOperator::assemble_with_space`] — complex scalar `ε`, or full
///   complex `(ε, ν)` tensors through its matched-UPML kernel — and the
///   walls on the 8-DOF p=2 tangential trace ([`crate::assembly::surface_p2`]).
///
/// # Errors
///
/// [`LossyCavityError::InvalidInput`] on length / value mismatches (gain
/// media, a non-positive or non-finite `omega_ref` with walls),
/// [`LossyCavityError::EmptyInterior`], and [`LossyCavityError::Assembly`]
/// for a foreign space, a wall that is not made of tet faces, or a wall
/// impedance that is singular at `omega_ref`.
pub fn assemble_lossy_pencil_on_space<B: Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: &LossyCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    walls: FrozenWalls<'_>,
    device: &B::Device,
) -> Result<LossyPencil, LossyCavityError> {
    use crate::driven::solve::{DrivenError, DrivenMaterials};
    if space.n_nodes() != mesh.n_nodes() || space.n_tets() != mesh.n_tets() {
        return Err(DrivenError::SpaceMeshMismatch {
            space_nodes: space.n_nodes(),
            space_tets: space.n_tets(),
            mesh_nodes: mesh.n_nodes(),
            mesh_tets: mesh.n_tets(),
        }
        .into());
    }
    if pec_interior_mask.len() != space.n_dofs() {
        return Err(LossyCavityError::InvalidInput(format!(
            "PEC mask has {} entries, the {:?} space has {} DOFs",
            pec_interior_mask.len(),
            space.order(),
            space.n_dofs()
        )));
    }
    validate_materials(mesh.n_tets(), materials)?;
    let coeffs: Vec<c64> = if walls.walls.is_empty() {
        Vec::new()
    } else {
        if !(walls.omega_ref.is_finite() && walls.omega_ref > 0.0) {
            return Err(LossyCavityError::InvalidInput(format!(
                "omega_ref must be finite and > 0 with impedance walls (got {})",
                walls.omega_ref
            )));
        }
        if walls.warn_rel_offset.is_nan() || walls.warn_rel_offset < 0.0 {
            return Err(LossyCavityError::InvalidInput(format!(
                "warn_rel_offset must be >= 0 (got {})",
                walls.warn_rel_offset
            )));
        }
        crate::driven::solve::validate_driven_surfaces(
            mesh,
            "impedance wall",
            walls.walls.iter().map(|w| w.triangles),
        )?;
        walls
            .walls
            .iter()
            .map(|w| w.model.weak_coefficient(walls.omega_ref))
            .collect::<Result<_, _>>()?
    };
    match space.order() {
        ElementOrder::P1 => {
            let (k, m) = assemble_lossy_pencil::<B>(mesh, materials, pec_interior_mask, device)?;
            if walls.walls.is_empty() {
                return Ok((k, m));
            }
            let mut remap = vec![usize::MAX; pec_interior_mask.len()];
            let mut n = 0usize;
            for (i, &keep) in pec_interior_mask.iter().enumerate() {
                if keep {
                    remap[i] = n;
                    n += 1;
                }
            }
            let mut tr = Vec::with_capacity(k.compute_nnz());
            for j in 0..k.ncols() {
                for (i, &v) in k.row_idx_of_col(j).zip(k.val_of_col(j)) {
                    tr.push(Triplet::new(i, j, v));
                }
            }
            for (w, &coeff) in walls.walls.iter().zip(&coeffs) {
                for (r, c, v) in crate::assembly::surface::assemble_surface_mass_triplets(
                    mesh,
                    w.triangles,
                    space.edges(),
                ) {
                    let (rr, cc) = (remap[r], remap[c]);
                    if rr != usize::MAX && cc != usize::MAX {
                        tr.push(Triplet::new(rr, cc, coeff * v));
                    }
                }
            }
            let k = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &tr).map_err(|e| {
                LossyCavityError::Eigen(EigenError::FaerGevd(format!("lossy K + walls: {e:?}")))
            })?;
            Ok((k, m))
        }
        ElementOrder::P2 => {
            let dm = match *materials {
                LossyCavityMaterials::Isotropic(eps) => DrivenMaterials::Scalar(eps),
                LossyCavityMaterials::Tensor { eps, nu } => DrivenMaterials::MatchedUpml {
                    epsilon_tensor: eps,
                    nu_tensor: nu,
                },
            };
            let op = crate::eigen::pec_cavity::assemble_eigen_operator_p2::<B>(
                space,
                mesh,
                dm,
                None,
                pec_interior_mask,
                walls.walls,
                device,
            )
            .map_err(|e| match e {
                DrivenError::EmptyInterior => LossyCavityError::EmptyInterior,
                other => LossyCavityError::Assembly(other),
            })?;
            let n = op.n_interior();
            let mut k_tr = Vec::with_capacity(op.rows().len());
            let mut m_tr = Vec::with_capacity(op.rows().len());
            for (((&r, &c), &kv), &mv) in op
                .rows()
                .iter()
                .zip(op.cols())
                .zip(op.k_vals())
                .zip(op.m_vals())
            {
                k_tr.push(Triplet::new(r, c, kv));
                m_tr.push(Triplet::new(r, c, mv));
            }
            for (i, &coeff) in coeffs.iter().enumerate() {
                let (trips, _model) = op.surface_mass_triplets(i);
                k_tr.extend(
                    trips
                        .into_iter()
                        .map(|(r, c, v)| Triplet::new(r, c, coeff * v)),
                );
            }
            let build = |tr: &[Triplet<usize, usize, c64>], what: &str| {
                SparseColMat::<usize, c64>::try_new_from_triplets(n, n, tr).map_err(|e| {
                    LossyCavityError::Eigen(EigenError::FaerGevd(format!("{what}: {e:?}")))
                })
            };
            Ok((
                build(&k_tr, "p=2 lossy cavity K")?,
                build(&m_tr, "p=2 lossy cavity M")?,
            ))
        }
    }
}

/// [`solve_lossy_cavity_modes`] on an order-pluggable [`HcurlSpace`] (issue
/// #871, Epic #836 Phase 2), optionally with impedance walls frozen at a
/// reference frequency ([`FrozenWalls`], [`assemble_lossy_pencil_on_space`]).
///
/// The residual-checked complex Lanczos, its null / overdamped filters, the
/// hole check and the selection are those of [`solve_lossy_cavity_modes`]
/// at both orders (p=1 without walls is bit-identical to it). At p=2 every
/// returned mode is additionally checked against the exact P2-Lagrange
/// gradient image ([`crate::eigen::hcurl_null`]): a returned gradient is
/// [`LossyCavityError::GradientModeReturned`], never a resonance.
///
/// With frequency-dependent walls the result also carries each mode's
/// offset from `omega_ref` ([`SpaceLossyCavityModes::wall_ref_offset`])
/// and a [`LossyCavityWarning::FrozenWallMismatch`] for every mode beyond
/// [`FrozenWalls::warn_rel_offset`] (see [`FrozenWalls`] for what the
/// offset costs in `Q` accuracy).
///
/// # Errors
///
/// Everything [`solve_lossy_cavity_modes`] and
/// [`assemble_lossy_pencil_on_space`] return, plus
/// [`LossyCavityError::GradientModeReturned`].
pub fn solve_lossy_cavity_modes_on_space<B: Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: &LossyCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    walls: FrozenWalls<'_>,
    settings: &LossyCavitySettings,
    device: &B::Device,
) -> Result<SpaceLossyCavityModes, LossyCavityError> {
    validate_settings(settings)?;
    let (k, m) = assemble_lossy_pencil_on_space::<B>(
        space,
        mesh,
        materials,
        pec_interior_mask,
        walls,
        device,
    )?;
    let wall_tris: Vec<&[[u32; 3]]> = walls.walls.iter().map(|w| w.triangles).collect();
    let null = GradientNullSpace::build(space, mesh, pec_interior_mask, &wall_tris);
    let modes = solve_lossy_pencil_modes(k.as_ref(), m.as_ref(), settings)?;
    let max_gradient_fraction = match space.order() {
        ElementOrder::P1 => None,
        ElementOrder::P2 => {
            let classifier = null.classifier(m.as_ref())?;
            let mut worst = 0.0_f64;
            for (index, md) in modes.modes.iter().enumerate() {
                let fraction = classifier.gradient_fraction(&md.vector);
                if fraction.is_nan() || fraction >= GRADIENT_FRACTION_CUT {
                    return Err(LossyCavityError::GradientModeReturned {
                        index,
                        lambda_re: md.lambda.re,
                        lambda_im: md.lambda.im,
                        fraction,
                    });
                }
                worst = worst.max(fraction);
            }
            Some(worst)
        }
    };
    let (wall_ref_offset, mut warnings) = frozen_wall_diagnostics(&walls, &modes);
    if let Some(h) = null.counts().loop_harmonics() {
        warnings.push(LossyCavityWarning::HarmonicFieldsPresent {
            b1: h.b1,
            loop_harmonics: h.count,
            exact: h.exact,
        });
    }
    Ok(SpaceLossyCavityModes {
        order: space.order(),
        modes,
        gradient_null: null.counts(),
        max_gradient_fraction,
        wall_ref_offset,
        warnings,
    })
}

/// Per-mode `|Re k₀ − ω_ref|/ω_ref` and the mismatch warnings
/// ([`FrozenWalls`]). `(None, [])` without a frequency-dependent wall.
fn frozen_wall_diagnostics(
    walls: &FrozenWalls<'_>,
    modes: &LossyCavityModes,
) -> (Option<Vec<f64>>, Vec<LossyCavityWarning>) {
    if !walls.is_frequency_dependent() {
        return (None, Vec::new());
    }
    let omega_ref = walls.omega_ref;
    let offsets: Vec<f64> = modes
        .modes
        .iter()
        .map(|md| (md.k0.re - omega_ref).abs() / omega_ref)
        .collect();
    let warnings = offsets
        .iter()
        .enumerate()
        .filter(|&(_, &d)| d > walls.warn_rel_offset)
        .map(
            |(index, &rel_offset)| LossyCavityWarning::FrozenWallMismatch {
                index,
                k0_re: modes.modes[index].k0.re,
                omega_ref,
                rel_offset,
                threshold: walls.warn_rel_offset,
            },
        )
        .collect();
    (Some(offsets), warnings)
}

/// [`solve_tagged_lossy_cavity_modes`] at a chosen [`ElementOrder`] (issue
/// #871): face-exact PEC mask of the named walls
/// ([`HcurlSpace::pec_interior_mask`]), solve by
/// [`solve_lossy_cavity_modes_on_space`]. Bit-identical to
/// [`solve_tagged_lossy_cavity_modes`] at [`ElementOrder::P1`].
///
/// # Errors
///
/// As [`solve_tagged_lossy_cavity_modes`] and
/// [`solve_lossy_cavity_modes_on_space`].
pub fn solve_tagged_lossy_cavity_modes_at_order<B: Backend>(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, c64)],
    order: ElementOrder,
    settings: &LossyCavitySettings,
    device: &B::Device,
) -> Result<SpaceLossyCavityModes, LossyCavityError> {
    let (eps_r, pec_tris) = tagged_eps_and_walls(tagged, pec_groups, materials)?;
    let lists: Vec<&[[u32; 3]]> = pec_tris.iter().map(Vec::as_slice).collect();
    let space = HcurlSpace::build(&tagged.mesh, order);
    let mask = space.pec_interior_mask(&tagged.mesh, &lists)?;
    solve_lossy_cavity_modes_on_space::<B>(
        &space,
        &tagged.mesh,
        &LossyCavityMaterials::Isotropic(&eps_r),
        &mask,
        FrozenWalls::NONE,
        settings,
        device,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::eigen::complex::ComplexEigenPair;
    use crate::eigen::pec_cavity::{PecCavitySettings, solve_pec_cavity_modes};
    use crate::mesh::cube_tet_mesh;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type B = TestBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    fn cube_mask(mesh: &TetMesh) -> Vec<bool> {
        crate::assembly::nedelec::cube_pec_interior_edges(mesh, 1.0).1
    }

    const TWO_PI2: f64 = 2.0 * std::f64::consts::PI * std::f64::consts::PI;

    /// Uniform lossy fill of the unit PEC cube: `λ = μ₀/ε_r` exactly, so
    /// `Im λ / Re λ = tan δ`, `Q = ½ cot(δ/2)`, `Im k > 0` (decay), and
    /// `|λ|·|ε_r|` equals the lossless eigenvalue of the same mesh.
    #[test]
    fn uniform_lossy_fill_matches_exact_tan_delta_relation() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let lossless = solve_pec_cavity_modes::<B>(
            &mesh,
            &vec![1.0; mesh.n_tets()],
            &mask,
            &PecCavitySettings::new(0.7 * TWO_PI2, 3),
            &device(),
        )
        .unwrap();
        // tan δ = 1/64 and 1/8 are dyadic, so exact even on f32 backends.
        for tan_d in [1.0 / 64.0, 0.125] {
            let eps = c64::new(1.0, -tan_d);
            let modes = solve_lossy_cavity_modes::<B>(
                &mesh,
                &LossyCavityMaterials::Isotropic(&vec![eps; mesh.n_tets()]),
                &mask,
                &LossyCavitySettings::new(0.7 * TWO_PI2, 3),
                &device(),
            )
            .unwrap();
            assert_eq!(modes.modes.len(), 3);
            let delta = tan_d.atan();
            let q_exact = 0.5 / (0.5 * delta).tan();
            for (lossy, real) in modes.modes.iter().zip(&lossless.modes) {
                let ratio = lossy.lambda.im / lossy.lambda.re;
                assert!(
                    (ratio - tan_d).abs() < 1e-8 * tan_d,
                    "Im/Re = {ratio}, want {tan_d}"
                );
                assert!(lossy.k0.im > 0.0 && lossy.k0.re > 0.0, "decay sign");
                let q = lossy.k0.re / (2.0 * lossy.k0.im);
                assert!((q - q_exact).abs() < 1e-8 * q_exact, "Q {q} vs {q_exact}");
                let mag = lossy.lambda.re.hypot(lossy.lambda.im) * eps.re.hypot(eps.im);
                assert!(
                    (mag - real.lambda).abs() < 1e-7 * real.lambda,
                    "|λ||ε| = {mag} vs lossless {}",
                    real.lambda
                );
                assert!(lossy.residual_rel < 1e-8);
                let k2 = lossy.k0 * lossy.k0;
                assert!((k2 - lossy.lambda).norm() < 1e-12 * lossy.lambda.norm());
            }
        }
    }

    /// A lossless (`Im ε = 0`) isotropic fill reproduces the real path.
    #[test]
    fn lossless_fill_matches_real_path() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let real = solve_pec_cavity_modes::<B>(
            &mesh,
            &vec![2.0; mesh.n_tets()],
            &mask,
            &PecCavitySettings::new(0.35 * TWO_PI2, 3),
            &device(),
        )
        .unwrap();
        let cplx = solve_lossy_cavity_modes::<B>(
            &mesh,
            &LossyCavityMaterials::Isotropic(&vec![c64::new(2.0, 0.0); mesh.n_tets()]),
            &mask,
            &LossyCavitySettings::new(0.35 * TWO_PI2, 3),
            &device(),
        )
        .unwrap();
        assert_eq!(real.n_interior, cplx.n_interior);
        for (a, b) in real.modes.iter().zip(&cplx.modes) {
            assert!((a.lambda - b.lambda.re).abs() < 1e-8 * a.lambda);
            assert!(b.lambda.im.abs() < 1e-8 * a.lambda);
        }
    }

    /// Identity-tensor materials equal the isotropic path.
    #[test]
    fn identity_tensor_matches_isotropic() {
        let mesh = cube_tet_mesh(2, 1.0);
        let mask = cube_mask(&mesh);
        let eps = c64::new(1.0, -0.125);
        let z = c64::new(0.0, 0.0);
        let mut eye = [[z; 3]; 3];
        for (i, row) in eye.iter_mut().enumerate() {
            row[i] = c64::new(1.0, 0.0);
        }
        let eps_t = vec![eye.map(|r| r.map(|v| v * eps)); mesh.n_tets()];
        let nu_t = vec![eye; mesh.n_tets()];
        let s = LossyCavitySettings::new(0.7 * TWO_PI2, 2);
        let iso = solve_lossy_cavity_modes::<B>(
            &mesh,
            &LossyCavityMaterials::Isotropic(&vec![eps; mesh.n_tets()]),
            &mask,
            &s,
            &device(),
        )
        .unwrap();
        let ten = solve_lossy_cavity_modes::<B>(
            &mesh,
            &LossyCavityMaterials::Tensor {
                eps: &eps_t,
                nu: &nu_t,
            },
            &mask,
            &s,
            &device(),
        )
        .unwrap();
        for (a, b) in iso.modes.iter().zip(&ten.modes) {
            assert!((a.lambda - b.lambda).norm() < 1e-8 * a.lambda.norm());
        }
    }

    #[test]
    fn principal_branch_convention() {
        let k = principal_k0(c64::new(3.0, 4.0));
        assert!((k - c64::new(2.0, 1.0)).norm() < 1e-14);
        let k = principal_k0(c64::new(3.0, -4.0));
        assert!((k - c64::new(2.0, -1.0)).norm() < 1e-14);
        let k = principal_k0(c64::new(-4.0, 0.0));
        assert!(k.re >= 0.0 && (k * k - c64::new(-4.0, 0.0)).norm() < 1e-14);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let mesh = cube_tet_mesh(2, 1.0);
        let mask = cube_mask(&mesh);
        let eps = vec![c64::new(1.0, -0.01); mesh.n_tets()];
        let dev = device();
        let run = |eps: &[c64], mask: &[bool], s: LossyCavitySettings| {
            solve_lossy_cavity_modes::<B>(
                &mesh,
                &LossyCavityMaterials::Isotropic(eps),
                mask,
                &s,
                &dev,
            )
        };
        let ok = LossyCavitySettings::new(10.0, 1);
        for bad in [0.0, -1.0, f64::NAN] {
            assert!(matches!(
                run(&eps, &mask, LossyCavitySettings { sigma: bad, ..ok }),
                Err(LossyCavityError::InvalidInput(_))
            ));
        }
        assert!(matches!(
            run(&eps, &mask, LossyCavitySettings { n_modes: 0, ..ok }),
            Err(LossyCavityError::InvalidInput(_))
        ));
        for bad in [0.0, f64::NAN] {
            assert!(matches!(
                run(
                    &eps,
                    &mask,
                    LossyCavitySettings {
                        residual_tol: bad,
                        ..ok
                    }
                ),
                Err(LossyCavityError::InvalidInput(_))
            ));
        }
        assert!(matches!(
            run(&eps[1..], &mask, ok),
            Err(LossyCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&eps, &mask[1..], ok),
            Err(LossyCavityError::InvalidInput(_))
        ));
        let mut gain = eps.clone();
        gain[0] = c64::new(1.0, 0.01);
        match run(&gain, &mask, ok) {
            Err(LossyCavityError::InvalidInput(msg)) => assert!(msg.contains("gain"), "{msg}"),
            other => panic!("expected gain rejection, got {other:?}"),
        }
        let mut neg = eps.clone();
        neg[0] = c64::new(-1.0, 0.0);
        assert!(matches!(
            run(&neg, &mask, ok),
            Err(LossyCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&eps, &vec![false; mask.len()], ok),
            Err(LossyCavityError::EmptyInterior)
        ));
        let z = [[c64::new(0.0, 0.0); 3]; 3];
        let short = vec![z; mesh.n_tets() - 1];
        let full = vec![z; mesh.n_tets()];
        assert!(matches!(
            solve_lossy_cavity_modes::<B>(
                &mesh,
                &LossyCavityMaterials::Tensor {
                    eps: &short,
                    nu: &full
                },
                &mask,
                &ok,
                &dev
            ),
            Err(LossyCavityError::InvalidInput(_))
        ));
    }

    /// PR #847 regression: a genuine mode nearest `σ` that is still
    /// unconverged when the extension hits its `2 · max_iters` cap must fail
    /// with `NotConverged`, never be silently replaced by a farther
    /// converged mode. On [`slow_mode_hole_pencil`] the checked solve
    /// returns two converged modes with no shortfall but without
    /// `λ* = 1.1 − 0.01j`, the eigenvalue nearest `σ = 1` (asserted in
    /// `checked_localized_hole_reports_capped_nearest_mode`). The
    /// shortfall-and-residual checks alone accept that set.
    #[test]
    fn capped_localized_mode_near_sigma_is_not_converged() {
        let (slow, k, m) = crate::eigen::complex::slow_mode_hole_pencil();
        let settings = LossyCavitySettings {
            max_iters: 14,
            tol: 1e-12,
            residual_tol: 1e-9,
            ..LossyCavitySettings::new(1.0, 2)
        };
        validate_settings(&settings).unwrap();
        match solve_lossy_pencil_modes(k.as_ref(), m.as_ref(), &settings) {
            Err(LossyCavityError::NotConverged {
                index,
                lambda_re,
                lambda_im,
                residual_rel,
                bound,
            }) => {
                assert!(
                    (c64::new(lambda_re, lambda_im) - slow).norm() < 1e-6,
                    "NotConverged must name λ*, got {lambda_re} {lambda_im:+}j"
                );
                // Ascending Re k₀ over {0.75, λ*}: λ* is second.
                assert_eq!(index, 1);
                assert_eq!(bound, 1e-9);
                assert!(residual_rel > bound, "ρ = {residual_rel:.3e}");
            }
            other => panic!("expected NotConverged for the hole, got {other:?}"),
        }
    }

    /// The other half of the PR #847 rule: a withheld value that is **not**
    /// localized (the bilinear Lanczos's spurious Ritz value, as in PR #833)
    /// is still skipped and the solve succeeds with the true nearest modes.
    /// The 16-step first pass on [`spurious_ritz_pencil`] holds the
    /// spurious `λ ≈ 1.090 + 0.149j` (ρ ≈ 2.2) among the 3 nearest `σ`; it
    /// never becomes a target, the run extends, and the 3 true eigenvalues
    /// nearest `σ` come back. The rule itself is pinned on a hand-built
    /// result in `localized_hole_skips_spurious_values`.
    #[test]
    fn spurious_withheld_value_still_succeeds() {
        let (lam, k, m) = crate::eigen::complex::spurious_ritz_pencil();
        let sigma = 1.0;
        let dist = |l: c64| (l.re - sigma).hypot(l.im);
        let settings = LossyCavitySettings {
            max_iters: 14,
            tol: 1e-12,
            residual_tol: 1e-9,
            ..LossyCavitySettings::new(sigma, 3)
        };
        validate_settings(&settings).unwrap();
        let modes = solve_lossy_pencil_modes(k.as_ref(), m.as_ref(), &settings).unwrap();
        eprintln!(
            "spurious pencil: {:?}, withheld {}, {} steps",
            modes.modes.iter().map(|m| m.lambda).collect::<Vec<_>>(),
            modes.n_withheld,
            modes.lanczos_steps
        );
        assert!(
            modes.lanczos_steps > 16,
            "the spurious value must extend the run"
        );
        let mut exact: Vec<c64> = lam.iter().copied().filter(|l| l.re > 0.0).collect();
        exact.sort_by(|a, b| dist(*a).total_cmp(&dist(*b)));
        assert_eq!(modes.modes.len(), 3);
        for e in &exact[..3] {
            assert!(
                modes.modes.iter().any(|md| (md.lambda - e).norm() < 1e-9),
                "eigenvalue {e} nearest σ missing"
            );
        }
    }

    /// Too small a Lanczos basis must fail with `NotConverged`, never
    /// return unconverged Ritz values.
    #[test]
    fn unconverged_lanczos_is_rejected() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let eps = vec![c64::new(1.0, -0.05); mesh.n_tets()];
        let settings = LossyCavitySettings {
            max_iters: 4,
            ..LossyCavitySettings::new(0.7 * TWO_PI2, 3)
        };
        match solve_lossy_cavity_modes::<B>(
            &mesh,
            &LossyCavityMaterials::Isotropic(&eps),
            &mask,
            &settings,
            &device(),
        ) {
            Err(LossyCavityError::NotConverged {
                index,
                residual_rel,
                bound,
                ..
            }) => {
                assert!(index < 3);
                assert_eq!(bound, LossyCavitySettings::DEFAULT_RESIDUAL_TOL);
                assert!(residual_rel.is_nan() || residual_rel > bound);
            }
            other => panic!("expected NotConverged, got {other:?}"),
        }
    }

    /// The post-solve degenerate-shift guard (#696) surfaces through
    /// this module's error type. `sigma > 0` is enforced up front, so the
    /// numerically-zero shift is reached with a tiny positive `σ` and the
    /// null filter disabled (`null_tol_rel = 0`), letting Lanczos collapse
    /// onto the gradient nullspace.
    #[test]
    fn degenerate_shift_propagates() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let eps = vec![c64::new(1.0, -0.05); mesh.n_tets()];
        let settings = LossyCavitySettings {
            null_tol_rel: 0.0,
            max_iters: 4,
            ..LossyCavitySettings::new(1e-300, 1)
        };
        match solve_lossy_cavity_modes::<B>(
            &mesh,
            &LossyCavityMaterials::Isotropic(&eps),
            &mask,
            &settings,
            &device(),
        ) {
            Err(LossyCavityError::Eigen(EigenError::DegenerateShift { .. })) => {}
            other => panic!("expected DegenerateShift, got {other:?}"),
        }
    }

    /// A seeded diagonal pencil shaped like the p=2 box of
    /// `golden3a_uniform_lossy_fill_exact_q_at_p2` (issue #968). It has the
    /// twelve physical modes `λ = p(1 + j tan δ)`, `tan δ = 1/8`, at the box
    /// values `p = 25.3, 37.3, 42.8, …`, plus `n_null` near-null values with
    /// `|λ| ≲ 10⁻⁴` (the gradient nullspace). `M` is a random positive
    /// diagonal and `K = ΛM`. Built from `+ − × ÷` only, like
    /// [`crate::eigen::complex::spurious_ritz_pencil`].
    fn near_null_ghost_pencil(
        seed: u64,
        n_null: usize,
    ) -> (Vec<c64>, SparseColMat<usize, c64>, SparseColMat<usize, c64>) {
        let lcg = |s: u64| {
            s.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
        };
        let mut st = lcg(seed + 1000);
        let mut rnd = || {
            st = lcg(st);
            ((st >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let tan_d = 0.125;
        let phys = [
            25.3, 37.3, 42.8, 52.0, 52.05, 54.2, 66.1, 70.5, 81.4, 81.5, 88.1, 98.0,
        ];
        let mut lam: Vec<c64> = phys.iter().map(|&p| c64::new(p, p * tan_d)).collect();
        for _ in 0..n_null {
            lam.push(c64::new(1e-4 * rnd(), 1e-4 * (rnd() - 0.5)));
        }
        let n = lam.len();
        let mut tk = Vec::with_capacity(n);
        let mut tm = Vec::with_capacity(n);
        for (i, l) in lam.iter().enumerate() {
            let w = c64::new(0.5 + rnd(), 0.0);
            tk.push(Triplet::new(i, i, *l * w));
            tm.push(Triplet::new(i, i, w));
        }
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
        (lam, k, m)
    }

    /// Issue #968 regression (CI flake of
    /// `golden3a_uniform_lossy_fill_exact_q_at_p2`). With `σ = 0.8 · 25.3`
    /// the near-null cluster (distance `≈ σ`) is nearer `σ` than the third
    /// requested mode (`42.8`, distance `≈ 23.1`). A short basis does not
    /// resolve the cluster and leaves a ghost Ritz value such as
    /// `λ ≈ 0.04 − 0.06j`, `ρ ≈ 0.1`. The ghost is above the null ceiling,
    /// `Re λ > 0`, and *localized* (its disc of radius `ρσ ≈ 2` misses `σ`),
    /// so it becomes a target, is never confirmed, and is withheld. The
    /// point test called it a hole, and the solve failed with
    /// `NotConverged` although the three physical modes had converged. Its
    /// disc contains `λ = 0`, so the disc test of [`eligible_hole`] does not
    /// count it, and the solve returns the three modes.
    #[test]
    fn near_null_ghost_is_not_a_hole() {
        let settings = LossyCavitySettings {
            max_iters: 12,
            ..LossyCavitySettings::new(0.8 * 25.3, 3)
        };
        validate_settings(&settings).unwrap();
        let s = &settings;
        let null_ceiling = s.null_tol_rel * s.sigma;
        let eligible = |l: c64| l.norm() > null_ceiling && l.re > 0.0;
        let dist = |l: c64| (l.re - s.sigma).hypot(l.im);
        let mut ghosts = 0;
        for seed in 0..64 {
            let (lam, k, m) = near_null_ghost_pencil(seed, 40);
            // What the old point test saw on the same checked solve.
            let solve = SparseComplexShiftInvertLanczos {
                sigma: s.sigma,
                max_iters: s.max_iters,
                tol: s.tol,
            }
            .smallest_eigenpairs_checked_filtered(
                k.as_ref(),
                m.as_ref(),
                s.n_modes,
                ConvergenceCheck {
                    residual_tol: s.residual_tol,
                    max_iters_cap: 2 * s.max_iters,
                    window: None,
                },
                &eligible,
            )
            .unwrap();
            let mut exact: Vec<c64> = lam.iter().copied().filter(|&l| eligible(l)).collect();
            exact.sort_by(|a, b| dist(*a).total_cmp(&dist(*b)));
            let reach = dist(exact[2]);
            let point = solve.localized_hole(s.sigma, reach, eligible);
            assert!(
                eligible_hole(&solve, s.sigma, reach, null_ceiling).is_none(),
                "seed {seed}: no physical mode is missing"
            );
            if let Some((ghost, rho)) = point {
                // The point test's hole is a ghost whose disc holds λ = 0.
                ghosts += 1;
                assert!(
                    ghost.norm() <= rho * ghost.norm().max(s.sigma),
                    "seed {seed}: the ghost {ghost} (ρ = {rho:.3e}) is not near-null"
                );
            }
            let modes = solve_lossy_pencil_modes(k.as_ref(), m.as_ref(), &settings)
                .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
            assert_eq!(modes.modes.len(), 3);
            for e in &exact[..3] {
                assert!(
                    modes
                        .modes
                        .iter()
                        .any(|md| (md.lambda - e).norm() < 1e-9 * e.norm()),
                    "seed {seed}: eigenvalue {e} nearest σ missing"
                );
            }
        }
        eprintln!("near-null ghost pencils: {ghosts} of 64 had a point-test hole");
        assert!(ghosts > 0, "no seed exercised the issue #968 path");
    }

    /// [`eligible_hole`] on hand-built results (issue #968). The ghost of
    /// the local repro (seed 11, `tan δ = 1/8`) and the value of the CI
    /// failure are localized at `σ ≈ 20.2`, but their discs reach `λ = 0`.
    /// A genuine unconverged mode near `σ` is still a hole. So is a disc
    /// that stays clear of both screened regions, while one that crosses
    /// `Re λ = 0` is not.
    #[test]
    fn eligible_hole_requires_an_eligible_disc() {
        let sigma = 0.8 * 25.29;
        let null_ceiling = LossyCavitySettings::DEFAULT_NULL_TOL_REL * sigma;
        let pair = |l: c64| ComplexEigenPair {
            lambda: l,
            vector: vec![c64::new(1.0, 0.0)],
        };
        let mut solve = CheckedComplexEigenpairs {
            pairs: vec![
                pair(c64::new(24.91, 3.11)),
                pair(c64::new(36.74, 4.59)),
                pair(c64::new(42.23, 5.28)),
            ],
            residuals: vec![1e-13; 3],
            rejected: vec![
                (c64::new(0.010071, 0.023264), 0.116),
                (c64::new(0.099698, 0.053001), 0.611),
            ],
            localized_withheld: Vec::new(),
            screened: Vec::new(),
            requested: 3,
            lanczos_steps: 320,
            extended: true,
        };
        let reach = (42.23_f64 - sigma).hypot(5.28);
        // Both are localized: the point test would report the nearer one.
        assert!(solve.localized_hole(sigma, reach, |_| true).is_some());
        assert_eq!(eligible_hole(&solve, sigma, reach, null_ceiling), None);
        // A genuine unconverged mode is a hole.
        let genuine = (c64::new(30.0, 3.7), 1e-3);
        solve.rejected.push(genuine);
        assert_eq!(
            eligible_hole(&solve, sigma, reach, null_ceiling),
            Some(genuine)
        );
        // A disc crossing Re λ = 0 (overdamped) is not; one clear of both
        // screened regions is.
        solve.rejected.pop();
        let crossing = (c64::new(0.5, 3.0), 0.05);
        solve.rejected.push(crossing);
        assert_eq!(eligible_hole(&solve, sigma, reach, null_ceiling), None);
        let clear = (c64::new(5.0, 3.0), 0.05);
        solve.rejected.push(clear);
        assert_eq!(
            eligible_hole(&solve, sigma, reach, null_ceiling),
            Some(clear)
        );
    }
}
