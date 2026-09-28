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
//! # Scope: lossless only
//!
//! There is no loss mechanism in this pencil (real `ε_r`, perfect
//! conductors, no ports, no absorbing boundary), so every eigenvalue is
//! real and a quality factor `Q` is **not defined** (it would be
//! infinite). Lossy / open cavities (complex `ε_r`, Leontovich walls,
//! Silver-Müller or UPML boundaries) need the complex quasimode pencils
//! in [`crate::eigen::complex`] and are out of scope here.
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

use crate::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_complex_epsilon_sparse,
};
use crate::assembly::p1::upload_mesh;
use crate::eigen::dense::EigenError;
use crate::eigen::lanczos::SparseShiftInvertLanczos;
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
}

impl PecCavitySettings {
    /// Default Lanczos basis size.
    pub const DEFAULT_MAX_ITERS: usize = 160;
    /// Default Lanczos tolerance.
    pub const DEFAULT_TOL: f64 = 1e-9;
    /// Default gradient-nullspace filter (relative to `σ`).
    pub const DEFAULT_NULL_TOL_REL: f64 = 1e-3;

    /// Settings with the defaults for everything but `sigma` / `n_modes`.
    pub fn new(sigma: f64, n_modes: usize) -> Self {
        Self {
            sigma,
            n_modes,
            max_iters: Self::DEFAULT_MAX_ITERS,
            tol: Self::DEFAULT_TOL,
            null_tol_rel: Self::DEFAULT_NULL_TOL_REL,
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
    /// Relative eigen-residual `‖K x − λ M x‖₂ / (|λ| ‖M x‖₂)`.
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
    let invalid = |m: String| PecCavityError::InvalidInput(m);
    if eps_r.len() != mesh.n_tets() {
        return Err(invalid(format!(
            "eps_r has {} entries, mesh has {} tets",
            eps_r.len(),
            mesh.n_tets()
        )));
    }
    if let Some(bad) = eps_r.iter().find(|e| !(e.is_finite() && **e > 0.0)) {
        return Err(invalid(format!("eps_r must be finite and > 0 (got {bad})")));
    }

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
    // with a purely real permittivity (the imaginary mass is identically 0
    // and discarded).
    let scatter = NedelecScatterMap::new(&tet_idx);
    let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);
    let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
        nodes_t, tets_t, &tet_sign, &scatter, &eps_c,
    );
    let k_vals: Vec<f64> = sys.k_vals.into_data().iter::<f64>().collect();
    let m_vals: Vec<f64> = sys.m_re_vals.into_data().iter::<f64>().collect();

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
/// were resolved (never a silently short list).
pub fn solve_pec_cavity_modes<B: Backend>(
    mesh: &TetMesh,
    eps_r: &[f64],
    pec_interior_mask: &[bool],
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<PecCavityModes, PecCavityError> {
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

    let (k, m) = assemble_lossless_pencil::<B>(mesh, eps_r, pec_interior_mask, device)?;
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
    let n_null_filtered = pairs.iter().filter(|p| p.lambda <= null_ceiling).count();
    let mut physical: Vec<_> = pairs
        .into_iter()
        .filter(|p| p.lambda > null_ceiling)
        .collect();
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

    let modes = physical
        .into_iter()
        .map(|p| {
            let kx = spmv(k.as_ref(), &p.vector);
            let mx = spmv(m.as_ref(), &p.vector);
            let r: Vec<f64> = kx.iter().zip(&mx).map(|(a, b)| a - p.lambda * b).collect();
            let residual_rel = norm2(&r) / (p.lambda.abs() * norm2(&mx));
            PecCavityMode {
                lambda: p.lambda,
                k0: p.lambda.sqrt(),
                residual_rel,
                vector: p.vector,
            }
        })
        .collect();
    Ok(PecCavityModes {
        modes,
        n_interior,
        n_null_filtered,
    })
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
    let lists: Vec<&[[u32; 3]]> = pec_tris.iter().map(Vec::as_slice).collect();
    let mask = pec_interior_mask_from_triangles(&tagged.mesh.edges(), &lists);
    solve_pec_cavity_modes::<B>(&tagged.mesh, &eps_r, &mask, settings, device)
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
        assert!(
            (l0 - two_pi2).abs() / two_pi2 < 0.25,
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
    }
}
