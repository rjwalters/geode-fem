//! Lossless **PEC-walled cavity eigenmodes on any tagged tet mesh**
//! (issue #681, Epic #680 Phase 1): materials + PEC surfaces in,
//! resonant wavenumbers / frequencies out.
//!
//! This is the generic "solve modes" entry point the `geode eigen` CLI
//! subcommand wraps. Unlike [`crate::eigen::cavity`] (hard-wired to a
//! canonical `[0, side]³` cube with a geometric PEC mask) it takes an
//! arbitrary [`TetMesh`], a per-tet real relative permittivity and a
//! per-edge PEC mask — or, via [`solve_tagged_pec_cavity_modes`], a
//! [`TaggedTetMesh`] plus Gmsh physical-group **names** for the PEC walls
//! and the dielectric regions.
//!
//! # The pencil
//!
//! First-order Nédélec (Whitney) edge elements, `μ_r = 1`:
//!
//! ```text
//! K x = λ M_ε x,   K_ij = ∫ ∇×N_i · ∇×N_j dV,   (M_ε)_ij = ∫ ε_r N_i · N_j dV,
//! λ = k₀²  (k₀ = ω/c in rad per mesh length unit)
//! ```
//!
//! restricted to the interior (non-PEC) edges. `K` and `M_ε` are
//! assembled **sparse** through the same pattern-aligned volume assembly
//! the driven solver uses
//! ([`crate::assembly::nedelec::assemble_global_nedelec_with_complex_epsilon_sparse`],
//! with `Im(ε_r) = 0`), so memory is `O(nnz)`, never `O(n_edges²)`. The
//! pencil is real symmetric (`K` PSD, `M_ε` SPD) and is solved with the
//! pure-Rust sparse shift-invert Lanczos
//! ([`SparseShiftInvertLanczos`], direct sparse-LU inner solve).
//!
//! # Diagonal anisotropic materials (issue #760)
//!
//! [`solve_pec_cavity_modes_with_materials`] with
//! [`PecCavityMaterials::Diagonal`] takes a per-tet real **diagonal**
//! permittivity `diag(ε_xx, ε_yy, ε_zz)` and inverse relative permeability
//! `diag(ν_xx, ν_yy, ν_zz)` (mesh axes, all `> 0`):
//!
//! ```text
//! K_ij = ∫ ∇×N_i · ν ∇×N_j dV,   M_ij = ∫ N_i · ε N_j dV.
//! ```
//!
//! The pencil is assembled through the matched-UPML full-tensor kernel
//! ([`assemble_global_nedelec_with_full_tensors_sparse`], off-diagonals and
//! imaginary parts zero). A positive diagonal reweighting keeps `K` PSD
//! (same gradient nullspace) and `M` SPD, so the real shift-invert Lanczos
//! applies unchanged.
//!
//! # Scope: lossless only
//!
//! There is no loss mechanism in this pencil (real `ε_r`, perfect
//! conductors, no ports, no absorbing boundary), so every eigenvalue is
//! real and a quality factor `Q` is **not defined** (it would be
//! infinite). Lossy / open cavities (complex `ε_r`, Leontovich walls,
//! Silver-Müller or UPML boundaries) need the complex quasimode pencils
//! in [`crate::eigen::complex`]: complex `ε_r` and fixed-frequency UPML
//! are handled by the sibling [`crate::eigen::lossy_cavity`] (issue #706);
//! frequency-dependent walls (Leontovich, Silver-Müller) make the operator
//! nonlinear in `λ` and remain out of scope for both.
//!
//! # Shift placement and the gradient nullspace
//!
//! The curl-curl `K` has a large nullspace (the discrete gradients,
//! `λ = 0`). The shift `σ = sigma` must be strictly positive so
//! `K − σM` is non-singular, and Lanczos converges the Ritz values
//! **closest to `σ`** first. Place `σ` just below the lowest mode of
//! interest: the near-zero gradient cluster is then filtered by
//! [`PecCavitySettings::null_tol_rel`] (any Ritz value `λ ≤ null_tol_rel·σ`
//! is classified as gradient nullspace and dropped), and the returned
//! modes are the `n_modes` physical eigenvalues closest to `σ`, sorted
//! ascending. A `σ` far above the band of interest makes Lanczos resolve
//! the modes around `σ` instead — that is a targeting choice, not an
//! error.

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_complex_epsilon_sparse,
    assemble_global_nedelec_with_full_tensors_sparse,
};
use crate::assembly::p1::upload_mesh;
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use crate::eigen::dense::EigenError;
use crate::eigen::hcurl_null::{
    GRADIENT_FRACTION_CUT, GradientNullCount, GradientNullSpace, real_to_complex,
};
use crate::eigen::lanczos::SparseShiftInvertLanczos;
use crate::eigen::transmon::LondonSurface;
use crate::elements::ElementOrder;
use crate::mesh::{TaggedTetMesh, TetMesh, pec_interior_mask_from_triangles};

/// Errors from the tagged-mesh lossless cavity eigensolve.
#[derive(Debug, thiserror::Error)]
pub enum PecCavityError {
    /// An input slice or setting is malformed (wrong length, non-finite,
    /// non-positive permittivity or shift, zero modes requested, …).
    #[error("invalid cavity eigen input: {0}")]
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
    /// The sparse eigensolve (LU factorization of `K − σM` or the
    /// Lanczos iteration) failed.
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
    /// The order-generic pencil assembly (issue #871) rejected its input:
    /// a wall or space that does not belong to the mesh, a London wall
    /// that is not made of tet faces, …
    #[error("cavity pencil assembly failed: {0}")]
    Assembly(#[from] crate::driven::solve::DrivenError),
    /// Null-count tripwire (issue #871): the gradient-fraction classifier
    /// flagged more Ritz pairs as gradients than the exact gradient null
    /// dimension allows ([`crate::eigen::hcurl_null`]). An M-orthonormal
    /// Ritz basis cannot hold more gradients than that, so the classifier
    /// or the pencil is broken; nothing is returned.
    #[error(
        "{classified} Ritz pairs were classified as gradients, but the exact gradient null \
         space has dimension {dim}"
    )]
    GradientNullExceeded {
        /// Ritz pairs the classifier flagged.
        classified: usize,
        /// The exact gradient null dimension.
        dim: usize,
    },
    /// A selected mode's relative eigen-residual exceeds
    /// [`PecCavitySettings::residual_tol`]: the Lanczos basis did not
    /// converge it, so its `λ` is not a trustworthy eigenvalue. Reports
    /// the **worst** offending mode.
    #[error(
        "eigensolve did not converge: mode {index} (λ = {lambda}) has relative residual \
         {residual_rel:e} > bound {bound:e} (raise max_iters, or move sigma closer to the \
         band of interest)"
    )]
    NotConverged {
        /// Index (in the ascending returned order) of the worst mode.
        index: usize,
        /// Its Ritz value.
        lambda: f64,
        /// Its relative residual (may be non-finite).
        residual_rel: f64,
        /// The acceptance bound it exceeded.
        bound: f64,
    },
}

/// Settings for [`solve_pec_cavity_modes`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PecCavitySettings {
    /// Shift `σ = k₀,target²` in `(rad / mesh length unit)²`, `> 0`. Place
    /// it just below the lowest mode of interest (see the module docs).
    pub sigma: f64,
    /// Number of physical modes to return (`≥ 1`).
    pub n_modes: usize,
    /// Lanczos basis size (outer iterations). More iterations resolve
    /// more — and more nearly-degenerate — modes at `O(iters² · n)` cost.
    pub max_iters: usize,
    /// Lanczos relative convergence tolerance.
    pub tol: f64,
    /// Gradient-nullspace filter: Ritz values `λ ≤ null_tol_rel · σ` are
    /// classified as the curl-free near-kernel and dropped.
    pub null_tol_rel: f64,
    /// Acceptance bound on each returned mode's relative eigen-residual
    /// [`PecCavityMode::residual_rel`]. If any selected mode exceeds it
    /// the solve fails with [`PecCavityError::NotConverged`] rather than
    /// returning unconverged Ritz values (default `1e-6`).
    pub residual_tol: f64,
}

impl PecCavitySettings {
    /// Default Lanczos basis size.
    pub const DEFAULT_MAX_ITERS: usize = 160;
    /// Default Lanczos tolerance.
    pub const DEFAULT_TOL: f64 = 1e-9;
    /// Default gradient-nullspace filter (relative to `σ`).
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

/// One physical cavity mode.
#[derive(Debug, Clone)]
pub struct PecCavityMode {
    /// Eigenvalue `λ = k₀²` in `(rad / mesh length unit)²`.
    pub lambda: f64,
    /// Resonant wavenumber `k₀ = √λ` in rad per mesh length unit.
    pub k0: f64,
    /// Relative eigen-residual `‖K x − λ M x‖₂ / (|λ| ‖M x‖₂)` (with `σ`
    /// in place of `|λ|` for an exactly-zero `λ`, which can only survive
    /// the null filter when `null_tol_rel = 0`). Always
    /// `≤ settings.residual_tol` for a returned mode.
    pub residual_rel: f64,
    /// `M_ε`-normalized eigenvector over the **interior** edges (the
    /// `true` entries of the PEC mask, in global edge order).
    pub vector: Vec<f64>,
}

/// Result of [`solve_pec_cavity_modes`].
#[derive(Debug, Clone)]
pub struct PecCavityModes {
    /// Physical modes, ascending in `λ`.
    pub modes: Vec<PecCavityMode>,
    /// Interior (non-PEC) edge-DOF count — the pencil dimension.
    pub n_interior: usize,
    /// Ritz values dropped as gradient nullspace (`λ ≤ null_tol_rel·σ`).
    pub n_null_filtered: usize,
}

/// Interior-reduced real sparse pencil `(K, M_ε)`.
pub type LosslessPencil = (SparseColMat<usize, f64>, SparseColMat<usize, f64>);

/// Per-tet material model of the lossless pencil.
#[derive(Debug, Clone, Copy)]
pub enum PecCavityMaterials<'a> {
    /// Isotropic real relative permittivity per tet (`> 0`), `μ_r = 1`.
    Isotropic(&'a [f64]),
    /// Diagonal anisotropic real materials per tet in mesh axes (issue
    /// #760): `eps` = `[ε_xx, ε_yy, ε_zz]` weights the mass, `nu` =
    /// `[1/μ_xx, 1/μ_yy, 1/μ_zz]` the curl-curl stiffness. Every component
    /// finite and `> 0`.
    Diagonal {
        /// Per-tet diagonal relative permittivity.
        eps: &'a [[f64; 3]],
        /// Per-tet diagonal inverse relative permeability.
        nu: &'a [[f64; 3]],
    },
}

fn validate_materials(
    n_tets: usize,
    materials: &PecCavityMaterials<'_>,
) -> Result<(), PecCavityError> {
    let invalid = |m: String| PecCavityError::InvalidInput(m);
    match materials {
        PecCavityMaterials::Isotropic(eps_r) => {
            if eps_r.len() != n_tets {
                return Err(invalid(format!(
                    "eps_r has {} entries, mesh has {n_tets} tets",
                    eps_r.len()
                )));
            }
            if let Some(bad) = eps_r.iter().find(|e| !(e.is_finite() && **e > 0.0)) {
                return Err(invalid(format!("eps_r must be finite and > 0 (got {bad})")));
            }
        }
        PecCavityMaterials::Diagonal { eps, nu } => {
            for (what, t) in [("eps", eps), ("nu", nu)] {
                if t.len() != n_tets {
                    return Err(invalid(format!(
                        "{what} diagonal has {} entries, mesh has {n_tets} tets",
                        t.len()
                    )));
                }
                if let Some(bad) = t.iter().flatten().find(|v| !(v.is_finite() && **v > 0.0)) {
                    return Err(invalid(format!(
                        "{what} diagonal components must be finite and > 0 (got {bad})"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Assemble the interior-reduced lossless pencil `(K, M_ε)` sparse.
///
/// `eps_r` is the per-tet real relative permittivity (`> 0`);
/// `pec_interior_mask` is the per-edge mask over `mesh.edges()` (`true` =
/// kept interior DOF), e.g. from
/// [`crate::mesh::pec_interior_mask_from_triangles`]. Interior DOFs are
/// renumbered contiguously in global edge order.
///
/// # Errors
///
/// [`PecCavityError::InvalidInput`] on length / value mismatches,
/// [`PecCavityError::EmptyInterior`] when no edge survives the mask.
pub fn assemble_lossless_pencil<B: Backend>(
    mesh: &TetMesh,
    eps_r: &[f64],
    pec_interior_mask: &[bool],
    device: &B::Device,
) -> Result<LosslessPencil, PecCavityError> {
    assemble_lossless_pencil_with_materials::<B>(
        mesh,
        &PecCavityMaterials::Isotropic(eps_r),
        pec_interior_mask,
        device,
    )
}

/// [`assemble_lossless_pencil`] for any [`PecCavityMaterials`] (issue
/// #760): the isotropic case is the scalar-ε kernel exactly as before; the
/// diagonal case feeds the full-tensor kernel real diagonal `ε` / `ν`.
///
/// # Errors
///
/// As [`assemble_lossless_pencil`].
pub fn assemble_lossless_pencil_with_materials<B: Backend>(
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    device: &B::Device,
) -> Result<LosslessPencil, PecCavityError> {
    let invalid = |m: String| PecCavityError::InvalidInput(m);
    validate_materials(mesh.n_tets(), materials)?;

    let tet_edges = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    if pec_interior_mask.len() != n_edges {
        return Err(invalid(format!(
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
        return Err(PecCavityError::EmptyInterior);
    }

    // Same pattern-aligned sparse volume assembly as the driven solver,
    // with purely real materials (the imaginary parts are identically 0
    // and discarded).
    let scatter = NedelecScatterMap::new(&tet_idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);
    let (k_vals, m_vals): (Vec<f64>, Vec<f64>) = match materials {
        PecCavityMaterials::Isotropic(eps_r) => {
            let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
            let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
                nodes_t, tets_t, &tet_sign, &scatter, &eps_c,
            );
            (
                sys.k_vals.into_data().iter::<f64>().collect(),
                sys.m_re_vals.into_data().iter::<f64>().collect(),
            )
        }
        PecCavityMaterials::Diagonal { eps, nu } => {
            let full = |d: &[f64; 3]| {
                let mut t = [[c64::new(0.0, 0.0); 3]; 3];
                for (k, row) in t.iter_mut().enumerate() {
                    row[k] = c64::new(d[k], 0.0);
                }
                t
            };
            let eps_t: Vec<[[c64; 3]; 3]> = eps.iter().map(full).collect();
            let nu_t: Vec<[[c64; 3]; 3]> = nu.iter().map(full).collect();
            let sys = assemble_global_nedelec_with_full_tensors_sparse(
                nodes_t, tets_t, &tet_sign, &scatter, &eps_t, &nu_t,
            );
            (
                sys.k_re_vals.into_data().iter::<f64>().collect(),
                sys.m_re_vals.into_data().iter::<f64>().collect(),
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
        k_tr.push(Triplet::new(rr, cc, k_vals[idx]));
        m_tr.push(Triplet::new(rr, cc, m_vals[idx]));
    }
    let build = |tr: &[Triplet<usize, usize, f64>], what: &str| {
        SparseColMat::<usize, f64>::try_new_from_triplets(n_interior, n_interior, tr)
            .map_err(|e| PecCavityError::Eigen(EigenError::FaerGevd(format!("{what}: {e:?}"))))
    };
    Ok((build(&k_tr, "cavity K")?, build(&m_tr, "cavity M")?))
}

/// `y = A x` for a column-major sparse `A`.
fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; a.nrows()];
    for (j, &xj) in x.iter().enumerate() {
        if xj == 0.0 {
            continue;
        }
        for (i, v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            y[i] += v * xj;
        }
    }
    y
}

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Solve the lossless PEC-cavity eigenproblem on `mesh`.
///
/// `eps_r`: per-tet real relative permittivity (`> 0`).
/// `pec_interior_mask`: per-edge mask over `mesh.edges()` (`true` = kept).
/// Returns the `settings.n_modes` physical modes closest to
/// `settings.sigma`, ascending (see the module docs for shift placement).
///
/// # Errors
///
/// [`PecCavityError::InvalidInput`] for malformed inputs/settings,
/// [`PecCavityError::Eigen`] if the factorization or Lanczos fails, and
/// [`PecCavityError::TooFewModes`] if fewer than `n_modes` physical modes
/// were resolved (never a silently short list), and
/// [`PecCavityError::NotConverged`] if any selected mode's relative
/// residual exceeds [`PecCavitySettings::residual_tol`] (never silently
/// unconverged eigenvalues).
pub fn solve_pec_cavity_modes<B: Backend>(
    mesh: &TetMesh,
    eps_r: &[f64],
    pec_interior_mask: &[bool],
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<PecCavityModes, PecCavityError> {
    solve_pec_cavity_modes_with_materials::<B>(
        mesh,
        &PecCavityMaterials::Isotropic(eps_r),
        pec_interior_mask,
        settings,
        device,
    )
}

/// [`solve_pec_cavity_modes`] for any [`PecCavityMaterials`] — in
/// particular diagonal anisotropic `ε` / `μ` (issue #760).
///
/// # Errors
///
/// As [`solve_pec_cavity_modes`].
pub fn solve_pec_cavity_modes_with_materials<B: Backend>(
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<PecCavityModes, PecCavityError> {
    validate_settings(settings)?;
    let (k, m) =
        assemble_lossless_pencil_with_materials::<B>(mesh, materials, pec_interior_mask, device)?;
    solve_assembled_pencil(&k, &m, settings)
}

/// Reject malformed [`PecCavitySettings`] (shared by the PEC and the
/// periodic cavity solves).
pub(crate) fn validate_settings(settings: &PecCavitySettings) -> Result<(), PecCavityError> {
    let s = settings;
    let invalid = |m: String| PecCavityError::InvalidInput(m);
    if !(s.sigma.is_finite() && s.sigma > 0.0) {
        return Err(invalid(format!(
            "sigma must be finite and > 0 (got {}); sigma = 0 makes K − σM singular",
            s.sigma
        )));
    }
    if s.n_modes == 0 {
        return Err(invalid("n_modes must be ≥ 1".into()));
    }
    if s.max_iters == 0 {
        return Err(invalid("max_iters must be ≥ 1".into()));
    }
    if !(s.tol.is_finite() && s.tol > 0.0) {
        return Err(invalid(format!(
            "tol must be finite and > 0 (got {})",
            s.tol
        )));
    }
    if !(s.null_tol_rel.is_finite() && s.null_tol_rel >= 0.0) {
        return Err(invalid(format!(
            "null_tol_rel must be finite and ≥ 0 (got {})",
            s.null_tol_rel
        )));
    }
    if !(s.residual_tol.is_finite() && s.residual_tol > 0.0) {
        return Err(invalid(format!(
            "residual_tol must be finite and > 0 (got {})",
            s.residual_tol
        )));
    }

    Ok(())
}

/// Shift-invert Lanczos on an assembled lossless pencil `(K, M)`, with the
/// gradient-null filter, the closest-to-`σ` selection and the residual gate
/// of [`solve_pec_cavity_modes`] (settings already validated). Shared by the
/// PEC cavity and the periodic cavity (issue #839) paths; eigenvectors are
/// over the pencil's own DOFs.
pub(crate) fn solve_assembled_pencil(
    k: &SparseColMat<usize, f64>,
    m: &SparseColMat<usize, f64>,
    settings: &PecCavitySettings,
) -> Result<PecCavityModes, PecCavityError> {
    Ok(solve_assembled_pencil_classified(k, m, settings, None)?.0)
}

/// [`solve_assembled_pencil`] with an optional **vector** null classifier
/// (issue #871): a Ritz pair is gradient null when `λ ≤ null_tol_rel·σ`
/// **or** `is_gradient(x)`. Returns the modes and the number of Ritz pairs
/// `is_gradient` flagged (`0` without a classifier). With `None` the
/// filter, selection and result are exactly [`solve_assembled_pencil`]'s.
pub(crate) fn solve_assembled_pencil_classified(
    k: &SparseColMat<usize, f64>,
    m: &SparseColMat<usize, f64>,
    settings: &PecCavitySettings,
    is_gradient: Option<&dyn Fn(&[f64]) -> bool>,
) -> Result<(PecCavityModes, usize), PecCavityError> {
    let s = settings;
    let n_interior = k.nrows();

    // Take every Ritz pair the Lanczos basis yields: the near-zero
    // gradient cluster can contribute several Ritz values that sit closer
    // to σ than the physical modes of interest, so the null filter must
    // run *before* the closest-to-σ truncation (the solver's own
    // `n_modes` cut ranks by |λ − σ| with the nulls still in).
    let request = s.max_iters.max(s.n_modes).min(n_interior);
    let solver = SparseShiftInvertLanczos {
        sigma: s.sigma,
        max_iters: s.max_iters,
        tol: s.tol,
        ..Default::default()
    };
    let pairs = solver.smallest_eigenpairs(k.as_ref(), m.as_ref(), request)?;

    let null_ceiling = s.null_tol_rel * s.sigma;
    let (n_null_filtered, n_gradient, mut physical) = match is_gradient {
        None => {
            let n_null_filtered = pairs.iter().filter(|p| p.lambda <= null_ceiling).count();
            let physical: Vec<_> = pairs
                .into_iter()
                .filter(|p| p.lambda > null_ceiling)
                .collect();
            (n_null_filtered, 0, physical)
        }
        Some(is_gradient) => {
            let flags: Vec<bool> = pairs.iter().map(|p| is_gradient(&p.vector)).collect();
            let n_gradient = flags.iter().filter(|&&g| g).count();
            let mut n_null_filtered = 0usize;
            let mut physical = Vec::with_capacity(pairs.len());
            for (p, g) in pairs.into_iter().zip(flags) {
                if g || p.lambda <= null_ceiling {
                    n_null_filtered += 1;
                } else {
                    physical.push(p);
                }
            }
            (n_null_filtered, n_gradient, physical)
        }
    };
    // Closest to σ first, then ascending.
    physical.sort_by(|a, b| {
        (a.lambda - s.sigma)
            .abs()
            .total_cmp(&(b.lambda - s.sigma).abs())
    });
    physical.truncate(s.n_modes);
    physical.sort_by(|a, b| a.lambda.total_cmp(&b.lambda));
    if physical.len() < s.n_modes {
        return Err(PecCavityError::TooFewModes {
            requested: s.n_modes,
            found: physical.len(),
            sigma: s.sigma,
        });
    }

    let modes: Vec<PecCavityMode> = physical
        .into_iter()
        .map(|p| {
            let kx = spmv(k.as_ref(), &p.vector);
            let mx = spmv(m.as_ref(), &p.vector);
            let r: Vec<f64> = kx.iter().zip(&mx).map(|(a, b)| a - p.lambda * b).collect();
            // An exactly-zero λ can only get here with `null_tol_rel = 0`;
            // normalize by σ (> 0, validated) instead of dividing by zero.
            let scale = if p.lambda != 0.0 {
                p.lambda.abs()
            } else {
                s.sigma
            };
            let residual_rel = norm2(&r) / (scale * norm2(&mx));
            PecCavityMode {
                lambda: p.lambda,
                k0: p.lambda.sqrt(),
                residual_rel,
                vector: p.vector,
            }
        })
        .collect();

    // Residual acceptance gate: Lanczos returns whatever Ritz pairs its
    // basis yields, converged or not. Reject the worst offender (a NaN
    // residual counts as the worst possible).
    let worst = modes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.residual_rel.is_nan() || m.residual_rel > s.residual_tol)
        .max_by(|(_, a), (_, b)| {
            let key = |x: f64| if x.is_nan() { f64::INFINITY } else { x };
            key(a.residual_rel).total_cmp(&key(b.residual_rel))
        });
    if let Some((index, m)) = worst {
        return Err(PecCavityError::NotConverged {
            index,
            lambda: m.lambda,
            residual_rel: m.residual_rel,
            bound: s.residual_tol,
        });
    }
    Ok((
        PecCavityModes {
            modes,
            n_interior,
            n_null_filtered,
        },
        n_gradient,
    ))
}

/// [`solve_pec_cavity_modes`] on a Gmsh-tagged mesh, binding by physical-
/// group **name**: `pec_groups` are dimension-2 surfaces whose triangle
/// edges are eliminated as PEC; `materials` maps dimension-3 region names
/// to a real relative permittivity (unlisted regions are vacuum,
/// `ε_r = 1`).
///
/// # Errors
///
/// [`PecCavityError::UnknownGroup`] for a name missing from the mesh in
/// the required dimension, [`PecCavityError::InvalidInput`] for a region
/// listed twice, plus everything [`solve_pec_cavity_modes`] returns.
pub fn solve_tagged_pec_cavity_modes<B: Backend>(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, f64)],
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<PecCavityModes, PecCavityError> {
    let (eps_r, pec_tris) = tagged_eps_and_walls(tagged, pec_groups, materials)?;
    let lists: Vec<&[[u32; 3]]> = pec_tris.iter().map(Vec::as_slice).collect();
    let mask = pec_interior_mask_from_triangles(&tagged.mesh.edges(), &lists);
    solve_pec_cavity_modes::<B>(&tagged.mesh, &eps_r, &mask, settings, device)
}

/// Resolve a tagged cavity's physical-group names: the per-tet real `ε_r`
/// (unlisted regions vacuum) and the PEC wall triangle lists.
fn tagged_eps_and_walls(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, f64)],
) -> Result<(Vec<f64>, Vec<Vec<[u32; 3]>>), PecCavityError> {
    let tag_of = |dim: i32, name: &str| {
        tagged
            .physical_group_tag(dim, name)
            .ok_or_else(|| PecCavityError::UnknownGroup {
                dim,
                name: name.to_string(),
            })
    };
    let mut eps_by_tag = std::collections::BTreeMap::new();
    for &(name, eps) in materials {
        if eps_by_tag.insert(tag_of(3, name)?, eps).is_some() {
            return Err(PecCavityError::InvalidInput(format!(
                "material for `{name}` listed more than once"
            )));
        }
    }
    let eps_r: Vec<f64> = tagged
        .tet_physical_tags
        .iter()
        .map(|t| eps_by_tag.get(t).copied().unwrap_or(1.0))
        .collect();
    let pec_tris = pec_groups
        .iter()
        .map(|name| Ok(tagged.triangles_with_tag(tag_of(2, name)?)))
        .collect::<Result<Vec<_>, PecCavityError>>()?;
    Ok((eps_r, pec_tris))
}

// ---------------------------------------------------------------------------
// Order-generic entry points (issue #871, Epic #836 Phase 2)
// ---------------------------------------------------------------------------

/// Result of the order-generic lossless solves
/// ([`solve_pec_cavity_modes_on_space`],
/// [`solve_tagged_pec_cavity_modes_at_order`]).
#[derive(Debug, Clone)]
pub struct SpaceCavityModes {
    /// Element order the pencil was actually assembled and solved at.
    pub order: ElementOrder,
    /// The modes, exactly as [`solve_pec_cavity_modes`] reports them.
    /// Eigenvectors are over the kept DOFs of the space's mask (in full-DOF
    /// order); scatter them with the mask to get a full
    /// [`HcurlSpace::n_dofs`] vector for [`HcurlSpace::field_at`].
    pub modes: PecCavityModes,
    /// The exact gradient null dimension of the pencil and its closed-form
    /// decomposition ([`crate::eigen::hcurl_null`]).
    pub gradient_null: GradientNullCount,
    /// Ritz pairs the gradient-fraction classifier flagged as curl-free
    /// (`≤ gradient_null.dim`, the null-count tripwire). `None` at p=1,
    /// whose null filter is the historical magnitude test only (the p=1
    /// path is bit-identical to [`solve_pec_cavity_modes`]).
    pub n_gradient_classified: Option<usize>,
}

/// The per-tet materials of a lossless pencil as driven-operator materials
/// (p=2 assembly through [`DrivenOperator::assemble_with_space`]).
enum OwnedDrivenMaterials {
    Scalar(Vec<c64>),
    Tensor(Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>),
}

impl OwnedDrivenMaterials {
    fn from_lossless(materials: &PecCavityMaterials<'_>) -> Self {
        let diag = |d: &[f64; 3]| {
            let mut t = [[c64::new(0.0, 0.0); 3]; 3];
            for (k, row) in t.iter_mut().enumerate() {
                row[k] = c64::new(d[k], 0.0);
            }
            t
        };
        match materials {
            PecCavityMaterials::Isotropic(eps) => {
                Self::Scalar(eps.iter().map(|&e| c64::new(e, 0.0)).collect())
            }
            PecCavityMaterials::Diagonal { eps, nu } => Self::Tensor(
                eps.iter().map(diag).collect(),
                nu.iter().map(diag).collect(),
            ),
        }
    }

    fn view(&self) -> DrivenMaterials<'_> {
        match self {
            Self::Scalar(e) => DrivenMaterials::Scalar(e),
            Self::Tensor(e, n) => DrivenMaterials::MatchedUpml {
                epsilon_tensor: e,
                nu_tensor: n,
            },
        }
    }
}

/// Assemble the ω-independent p=2 operator on `space` for an eigen pencil
/// (no ports, no source), mapping the driven errors onto the cavity ones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble_eigen_operator_p2<B: Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    pec_interior_mask: &[bool],
    surfaces: &[SurfaceImpedanceBc<'_>],
    device: &B::Device,
) -> Result<DrivenOperator, DrivenError> {
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    DrivenOperator::assemble_with_space::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        &DrivenBcs { pec_interior_mask },
        &[],
        surfaces,
        DrivenSource::Constant(&source),
        device,
    )
}

/// Reject malformed London walls up front (the p=1 kernel panics on them).
fn validate_london(mesh: &TetMesh, london: &[LondonSurface<'_>]) -> Result<(), PecCavityError> {
    if let Some(bad) = london
        .iter()
        .find(|w| !(w.lambda_l.is_finite() && w.lambda_l > 0.0))
    {
        return Err(PecCavityError::InvalidInput(format!(
            "London lambda_l must be finite and > 0 (got {}); the λ_L = 0 PEC limit is the PEC \
             mask",
            bad.lambda_l
        )));
    }
    crate::driven::solve::validate_driven_surfaces(
        mesh,
        "London wall",
        london.iter().map(|w| w.triangles),
    )?;
    Ok(())
}

/// [`assemble_lossless_pencil_with_materials`] on an order-pluggable
/// [`HcurlSpace`] (issue #871, Epic #836 Phase 2), optionally with London
/// superconducting walls `λ_L⁻¹ S_Γ` on the K side
/// ([`crate::eigen::transmon::LondonSurface`]; real, positive semi-definite
/// and frequency-independent, so the pencil stays real symmetric and
/// linear in `λ`).
///
/// `pec_interior_mask` is over `space.n_dofs()` — build it with
/// [`HcurlSpace::pec_interior_mask`] (face-exact, #780). Interior DOFs are
/// renumbered contiguously in full-DOF order.
///
/// - **p=1, no London walls:** calls
///   [`assemble_lossless_pencil_with_materials`] verbatim (bit-identical).
///   With London walls, their Whitney surface masses
///   ([`crate::eigen::transmon::LondonSurface::k_triplets`]) are added to
///   that `K` on the kept edges.
/// - **p=2:** the 20-DOF element through the host-side p=2 assembly of
///   [`DrivenOperator::assemble_with_space`] (degree-4 rule, exact for
///   per-tet-constant materials; diagonal `ε`/`ν` through its full-tensor
///   kernel), and the London masses on the 8-DOF p=2 tangential trace
///   ([`crate::assembly::surface_p2`], #857).
///
/// # Errors
///
/// [`PecCavityError::InvalidInput`] on length / value mismatches (including
/// a non-positive `λ_L`), [`PecCavityError::EmptyInterior`] when no DOF
/// survives the mask, [`PecCavityError::Assembly`] for a space built on
/// another mesh or a London wall that is not made of tet faces.
pub fn assemble_lossless_pencil_on_space<B: Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    london: &[LondonSurface<'_>],
    device: &B::Device,
) -> Result<LosslessPencil, PecCavityError> {
    check_space(space, mesh)?;
    if pec_interior_mask.len() != space.n_dofs() {
        return Err(PecCavityError::InvalidInput(format!(
            "PEC mask has {} entries, the {:?} space has {} DOFs",
            pec_interior_mask.len(),
            space.order(),
            space.n_dofs()
        )));
    }
    validate_london(mesh, london)?;
    match space.order() {
        ElementOrder::P1 => {
            let (k, m) = assemble_lossless_pencil_with_materials::<B>(
                mesh,
                materials,
                pec_interior_mask,
                device,
            )?;
            if london.is_empty() {
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
            let mut extra = Vec::new();
            for wall in london {
                for (r, c, v) in wall.k_triplets(mesh, space.edges()) {
                    let (rr, cc) = (remap[r], remap[c]);
                    if rr != usize::MAX && cc != usize::MAX {
                        extra.push((rr, cc, v));
                    }
                }
            }
            Ok((add_triplets(&k, &extra, "cavity K + London")?, m))
        }
        ElementOrder::P2 => {
            validate_materials(mesh.n_tets(), materials)?;
            let owned = OwnedDrivenMaterials::from_lossless(materials);
            let surfaces: Vec<SurfaceImpedanceBc<'_>> = london
                .iter()
                .map(|w| SurfaceImpedanceBc {
                    triangles: w.triangles,
                    model: SurfaceImpedanceModel::London {
                        lambda_l: w.lambda_l,
                    },
                })
                .collect();
            let op = assemble_eigen_operator_p2::<B>(
                space,
                mesh,
                owned.view(),
                None,
                pec_interior_mask,
                &surfaces,
                device,
            )
            .map_err(empty_or_assembly)?;
            let n = op.n_interior();
            let mut k_tr = Vec::with_capacity(op.rows().len());
            let mut m_tr = Vec::with_capacity(op.rows().len());
            for (((&r, &c), kv), mv) in op
                .rows()
                .iter()
                .zip(op.cols())
                .zip(op.k_vals())
                .zip(op.m_vals())
            {
                k_tr.push(Triplet::new(r, c, kv.re));
                m_tr.push(Triplet::new(r, c, mv.re));
            }
            for (i, wall) in london.iter().enumerate() {
                let (trips, _model) = op.surface_mass_triplets(i);
                let w = 1.0 / wall.lambda_l;
                k_tr.extend(trips.into_iter().map(|(r, c, v)| Triplet::new(r, c, w * v)));
            }
            let build = |tr: &[Triplet<usize, usize, f64>], what: &str| {
                SparseColMat::<usize, f64>::try_new_from_triplets(n, n, tr).map_err(|e| {
                    PecCavityError::Eigen(EigenError::FaerGevd(format!("{what}: {e:?}")))
                })
            };
            Ok((build(&k_tr, "p=2 cavity K")?, build(&m_tr, "p=2 cavity M")?))
        }
    }
}

/// `DrivenError::EmptyInterior` is the cavity's own `EmptyInterior`.
fn empty_or_assembly(e: DrivenError) -> PecCavityError {
    match e {
        DrivenError::EmptyInterior => PecCavityError::EmptyInterior,
        other => PecCavityError::Assembly(other),
    }
}

/// [`DrivenError::SpaceMeshMismatch`] unless `space` was built on `mesh`.
fn check_space(space: &HcurlSpace, mesh: &TetMesh) -> Result<(), DrivenError> {
    if space.n_nodes() == mesh.n_nodes() && space.n_tets() == mesh.n_tets() {
        Ok(())
    } else {
        Err(DrivenError::SpaceMeshMismatch {
            space_nodes: space.n_nodes(),
            space_tets: space.n_tets(),
            mesh_nodes: mesh.n_nodes(),
            mesh_tets: mesh.n_tets(),
        })
    }
}

/// `a + Σ extra` as a new sparse matrix (duplicates summed).
fn add_triplets(
    a: &SparseColMat<usize, f64>,
    extra: &[(usize, usize, f64)],
    what: &str,
) -> Result<SparseColMat<usize, f64>, PecCavityError> {
    let mut tr = Vec::with_capacity(a.compute_nnz() + extra.len());
    for j in 0..a.ncols() {
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            tr.push(Triplet::new(i, j, v));
        }
    }
    tr.extend(extra.iter().map(|&(r, c, v)| Triplet::new(r, c, v)));
    SparseColMat::<usize, f64>::try_new_from_triplets(a.nrows(), a.ncols(), &tr)
        .map_err(|e| PecCavityError::Eigen(EigenError::FaerGevd(format!("{what}: {e:?}"))))
}

/// [`solve_pec_cavity_modes_with_materials`] on an order-pluggable
/// [`HcurlSpace`] (issue #871, Epic #836 Phase 2), optionally with London
/// walls (see [`assemble_lossless_pencil_on_space`]).
///
/// - **p=1** (no London walls): the pencil, the Lanczos solve, the null
///   filter and the selection are exactly
///   [`solve_pec_cavity_modes_with_materials`]'s, so `modes` is
///   bit-identical to it.
/// - **p=2:** the same Lanczos solve and selection, with the null filter
///   extended by the exact **gradient-fraction classifier**
///   ([`crate::eigen::hcurl_null`]): a Ritz pair is null when
///   `λ ≤ null_tol_rel·σ` or when it lies in the P2-Lagrange gradient image
///   (fraction `≥ ½`). A gradient Ritz value that drifts above the
///   magnitude filter is therefore never returned as a mode, and a physical
///   mode is never dropped for being small. The number of classified pairs
///   is checked against the exact null dimension (the tripwire).
///
/// # Errors
///
/// Everything [`solve_pec_cavity_modes`] and
/// [`assemble_lossless_pencil_on_space`] return, plus
/// [`PecCavityError::GradientNullExceeded`] when the tripwire fires.
pub fn solve_pec_cavity_modes_on_space<B: Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    pec_interior_mask: &[bool],
    london: &[LondonSurface<'_>],
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<SpaceCavityModes, PecCavityError> {
    validate_settings(settings)?;
    let (k, m) = assemble_lossless_pencil_on_space::<B>(
        space,
        mesh,
        materials,
        pec_interior_mask,
        london,
        device,
    )?;
    let walls: Vec<&[[u32; 3]]> = london.iter().map(|w| w.triangles).collect();
    let null = GradientNullSpace::build(space, mesh, pec_interior_mask, &walls);
    match space.order() {
        ElementOrder::P1 => Ok(SpaceCavityModes {
            order: ElementOrder::P1,
            modes: solve_assembled_pencil(&k, &m, settings)?,
            gradient_null: null.counts(),
            n_gradient_classified: None,
        }),
        ElementOrder::P2 => {
            let m_c = real_to_complex(m.as_ref())?;
            let classifier = null.classifier(m_c.as_ref())?;
            let is_gradient =
                |x: &[f64]| classifier.gradient_fraction_real(x) >= GRADIENT_FRACTION_CUT;
            let (modes, classified) =
                solve_assembled_pencil_classified(&k, &m, settings, Some(&is_gradient))?;
            if classified > null.dim() {
                return Err(PecCavityError::GradientNullExceeded {
                    classified,
                    dim: null.dim(),
                });
            }
            Ok(SpaceCavityModes {
                order: ElementOrder::P2,
                modes,
                gradient_null: null.counts(),
                n_gradient_classified: Some(classified),
            })
        }
    }
}

/// [`solve_tagged_pec_cavity_modes`] at a chosen [`ElementOrder`] (issue
/// #871): the PEC mask is the face-exact
/// [`HcurlSpace::pec_interior_mask`] of the named walls, the solve is
/// [`solve_pec_cavity_modes_on_space`]. At [`ElementOrder::P1`] the modes
/// are bit-identical to [`solve_tagged_pec_cavity_modes`].
///
/// # Errors
///
/// As [`solve_tagged_pec_cavity_modes`] and
/// [`solve_pec_cavity_modes_on_space`].
pub fn solve_tagged_pec_cavity_modes_at_order<B: Backend>(
    tagged: &TaggedTetMesh,
    pec_groups: &[&str],
    materials: &[(&str, f64)],
    order: ElementOrder,
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<SpaceCavityModes, PecCavityError> {
    let (eps_r, pec_tris) = tagged_eps_and_walls(tagged, pec_groups, materials)?;
    let lists: Vec<&[[u32; 3]]> = pec_tris.iter().map(Vec::as_slice).collect();
    let space = HcurlSpace::build(&tagged.mesh, order);
    let mask = space.pec_interior_mask(&tagged.mesh, &lists)?;
    solve_pec_cavity_modes_on_space::<B>(
        &space,
        &tagged.mesh,
        &PecCavityMaterials::Isotropic(&eps_r),
        &mask,
        &[],
        settings,
        device,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type B = TestBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    /// Geometric PEC mask of the unit cube's six faces.
    fn cube_mask(mesh: &TetMesh) -> Vec<bool> {
        crate::assembly::nedelec::cube_pec_interior_edges(mesh, 1.0).1
    }

    /// Vacuum unit cube: the lowest physical mode is the 3-fold
    /// degenerate `TE101`-family at `λ = 2π²`; a uniform `ε_r` scales
    /// every eigenvalue by `1/ε_r`.
    #[test]
    fn unit_cube_lowest_mode_and_epsilon_scaling() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);

        let vac = vec![1.0; mesh.n_tets()];
        let settings = PecCavitySettings::new(0.7 * two_pi2, 3);
        let modes = solve_pec_cavity_modes::<B>(&mesh, &vac, &mask, &settings, &device()).unwrap();
        assert_eq!(modes.modes.len(), 3);
        let l0 = modes.modes[0].lambda;
        eprintln!(
            "unit cube n=3: lowest λ = {l0:.6} vs 2π² = {two_pi2:.6} (rel err {:.4e}, {} interior edges)",
            (l0 - two_pi2).abs() / two_pi2,
            modes.n_interior
        );
        // 10% bar (was 25%; issue #771): measured 6.6% with the face-exact
        // PEC mask (117 interior edges). The pre-#771 node-rule mask also
        // pinned the 24 interior chords along the cube's edges (93 interior
        // edges), stiffening the pencil to a 19.1% error.
        assert!(
            (l0 - two_pi2).abs() / two_pi2 < 0.10,
            "lowest λ = {l0}, want ≈ 2π² = {two_pi2}"
        );
        for m in &modes.modes {
            assert!(m.residual_rel < 1e-6, "residual {}", m.residual_rel);
            assert!((m.k0 * m.k0 - m.lambda).abs() < 1e-9 * m.lambda);
        }

        // ε_r = 4 everywhere halves k₀ (quarters λ); shift scaled to match.
        let eps4 = vec![4.0; mesh.n_tets()];
        let settings4 = PecCavitySettings::new(0.7 * two_pi2 / 4.0, 3);
        let modes4 =
            solve_pec_cavity_modes::<B>(&mesh, &eps4, &mask, &settings4, &device()).unwrap();
        for (a, b) in modes.modes.iter().zip(&modes4.modes) {
            assert!(
                (a.lambda / 4.0 - b.lambda).abs() < 1e-6 * a.lambda,
                "{} vs {}",
                a.lambda,
                b.lambda
            );
        }
    }

    /// Diagonal materials `ε = (e, e, e)`, `ν = (1, 1, 1)` reproduce the
    /// isotropic scalar path to round-off (issue #760).
    #[test]
    fn isotropic_diagonal_matches_scalar_path() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let eps: Vec<f64> = (0..mesh.n_tets())
            .map(|t| 1.0 + 0.5 * (t % 4) as f64)
            .collect();
        let eps_d: Vec<[f64; 3]> = eps.iter().map(|&e| [e; 3]).collect();
        let nu_d = vec![[1.0; 3]; mesh.n_tets()];
        let settings = PecCavitySettings::new(4.0, 4);
        let iso = solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device()).unwrap();
        let diag = solve_pec_cavity_modes_with_materials::<B>(
            &mesh,
            &PecCavityMaterials::Diagonal {
                eps: &eps_d,
                nu: &nu_d,
            },
            &mask,
            &settings,
            &device(),
        )
        .unwrap();
        assert_eq!(iso.n_interior, diag.n_interior);
        for (a, b) in iso.modes.iter().zip(&diag.modes) {
            assert!(
                (a.lambda - b.lambda).abs() <= 1e-9 * a.lambda,
                "{} vs {}",
                a.lambda,
                b.lambda
            );
        }
    }

    /// A diagonal positive `ν` / `ε` keeps the pencil's structure (issue
    /// #760): `K` annihilates discrete gradients (the curl-curl nullspace
    /// is unchanged), and the AMS-style proxy `K + ω²M` is SPD (Cholesky
    /// succeeds) — the property `solve_ams` relies on for `Re K(ν)`.
    #[test]
    fn diagonal_materials_keep_gradient_nullspace_and_spd_proxy() {
        let mesh = cube_tet_mesh(2, 1.0);
        let n_edges = mesh.edges().len();
        let mask = vec![true; n_edges];
        let eps_d = vec![[1.0, 2.5, 4.0]; mesh.n_tets()];
        let nu_d = vec![[0.25, 1.0, 3.0]; mesh.n_tets()];
        let (k, m) = assemble_lossless_pencil_with_materials::<B>(
            &mesh,
            &PecCavityMaterials::Diagonal {
                eps: &eps_d,
                nu: &nu_d,
            },
            &mask,
            &device(),
        )
        .unwrap();
        // Gradient of an arbitrary nodal field.
        let phi: Vec<f64> = mesh
            .nodes
            .iter()
            .map(|p| (1.3 * p[0]).sin() + p[1] * p[2] - 0.7 * p[2])
            .collect();
        let g: Vec<f64> = mesh
            .edges()
            .iter()
            .map(|&[a, b]| phi[b as usize] - phi[a as usize])
            .collect();
        let kg = spmv(k.as_ref(), &g);
        let mg = spmv(m.as_ref(), &g);
        assert!(
            norm2(&kg) <= 1e-10 * norm2(&mg),
            "‖K g‖ = {:e} vs ‖M g‖ = {:e}",
            norm2(&kg),
            norm2(&mg)
        );
        let dense = |a: &SparseColMat<usize, f64>| a.to_dense();
        let (kd, md) = (dense(&k), dense(&m));
        let proxy = &kd + &md;
        assert!(
            proxy.llt(faer::Side::Lower).is_ok(),
            "K(ν) + M(ε) is not SPD"
        );
        assert!(md.llt(faer::Side::Lower).is_ok(), "M(ε) is not SPD");
    }

    /// Non-positive / mis-sized diagonal materials are rejected.
    #[test]
    fn invalid_diagonal_materials_are_rejected() {
        let mesh = cube_tet_mesh(2, 1.0);
        let mask = cube_mask(&mesh);
        let n = mesh.n_tets();
        let ok_eps = vec![[1.0; 3]; n];
        let ok_nu = vec![[1.0; 3]; n];
        let mut bad = ok_eps.clone();
        bad[0][2] = 0.0;
        for (eps, nu) in [
            (&bad, &ok_nu),
            (&ok_eps, &bad),
            (&ok_eps[1..].to_vec(), &ok_nu),
        ] {
            assert!(matches!(
                assemble_lossless_pencil_with_materials::<B>(
                    &mesh,
                    &PecCavityMaterials::Diagonal { eps, nu },
                    &mask,
                    &device(),
                ),
                Err(PecCavityError::InvalidInput(_))
            ));
        }
    }

    /// Issue #740 regression: the forward eigenvalue is smooth in `ε` at
    /// f64 precision. A uniform `ε_r ×= 1.0001` must scale `λ` by exactly
    /// `1/1.0001` up to solver round-off. With the old f32 `ε` upload the
    /// ratio came out `f32(1.0001) = 1.0001000165939…` (a `1.66e-8`
    /// relative excess, which a central FD at relative step `1e-4`
    /// amplifies to `1.66e-4`).
    #[test]
    fn uniform_epsilon_scaling_is_exact_at_f64() {
        if std::mem::size_of::<<B as BackendTypes>::FloatElem>() != 8 {
            return; // f32-class backend: the precision claim is f64-only.
        }
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let settings = PecCavitySettings::new(0.7 * 2.0 * std::f64::consts::PI.powi(2), 3);
        let solve = |e: f64| {
            let eps = vec![e; mesh.n_tets()];
            solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device()).unwrap()
        };
        let (base, scaled) = (solve(1.0), solve(1.0001));
        for (a, b) in base.modes.iter().zip(&scaled.modes) {
            let ratio = a.lambda / b.lambda;
            let rel = (ratio / 1.0001 - 1.0).abs();
            assert!(
                rel < 1e-11,
                "λ0/λ = {ratio:.15} vs 1.0001 (rel {rel:.3e}): ε is being truncated \
                 below f64 on the way into the pencil (issue #740)"
            );
        }
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let mesh = cube_tet_mesh(2, 1.0);
        let mask = cube_mask(&mesh);
        let eps = vec![1.0; mesh.n_tets()];
        let dev = device();
        let run = |eps: &[f64], mask: &[bool], s: PecCavitySettings| {
            solve_pec_cavity_modes::<B>(&mesh, eps, mask, &s, &dev)
        };
        let ok = PecCavitySettings::new(10.0, 1);
        assert!(matches!(
            run(&eps, &mask, PecCavitySettings { sigma: 0.0, ..ok }),
            Err(PecCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&eps, &mask, PecCavitySettings { n_modes: 0, ..ok }),
            Err(PecCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&eps[1..], &mask, ok),
            Err(PecCavityError::InvalidInput(_))
        ));
        let mut neg = eps.clone();
        neg[0] = -1.0;
        assert!(matches!(
            run(&neg, &mask, ok),
            Err(PecCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&eps, &vec![false; mask.len()], ok),
            Err(PecCavityError::EmptyInterior)
        ));
        for bad in [0.0, -1e-6, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                run(
                    &eps,
                    &mask,
                    PecCavitySettings {
                        residual_tol: bad,
                        ..ok
                    }
                ),
                Err(PecCavityError::InvalidInput(_))
            ));
        }
    }

    /// A Lanczos basis far too small to converge the requested modes must
    /// fail with `NotConverged` instead of returning unconverged Ritz
    /// values as eigenvalues.
    #[test]
    fn unconverged_lanczos_is_rejected() {
        let mesh = cube_tet_mesh(3, 1.0);
        let mask = cube_mask(&mesh);
        let eps = vec![1.0; mesh.n_tets()];
        let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);
        let settings = PecCavitySettings {
            max_iters: 4,
            ..PecCavitySettings::new(0.7 * two_pi2, 3)
        };
        match solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device()) {
            Err(PecCavityError::NotConverged {
                index,
                residual_rel,
                bound,
                ..
            }) => {
                assert!(index < 3);
                assert_eq!(bound, PecCavitySettings::DEFAULT_RESIDUAL_TOL);
                assert!(
                    residual_rel.is_nan() || residual_rel > bound,
                    "residual {residual_rel}"
                );
            }
            other => panic!("expected NotConverged, got {other:?}"),
        }
    }
}
