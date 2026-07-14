//! Transmon eigenmode solve with a **Josephson junction as a lumped
//! reactive shunt surface term** (Epic #476 Phase B, issue #492).
//!
//! # The reactive lumped-shunt substitution
//!
//! The driven lumped port
//! ([`crate::driven::ports::LumpedPort`], Epic #193/#202)
//! contributes a Robin term `A(ω) += (jω/Z_s) S_Γ` to the driven system,
//! with `Z_s = Z_lumped · w/ℓ` the uniform-port surface impedance and
//! `S_Γ[i,j] = ∮_Γ N_i · N_j dS` the real symmetric PSD tangential
//! surface mass ([`crate::driven::ports::assemble_port_surface_mass`]).
//! Only resistive `Z = R` is supported there, in the DRIVEN path.
//!
//! Substituting the **reactive** lumped impedances of the Josephson
//! junction (modeled as a linear inductor `L` in parallel with a junction
//! capacitance `C`) into the same Robin term keeps everything **real**:
//!
//! - **Inductor** `Z = jωL`: `jω/Z_s = jω · ℓ/(jωL·w) = ℓ/(L·w)` —
//!   frequency-independent and REAL. It is a **stiffness**, adding to `K`:
//!
//!   ```text
//!   K_port = (ℓ / (w · L̃)) · S_Γ.
//!   ```
//!
//! - **Capacitor** `Z = 1/(jωC)`: `jω/Z_s = −ω² · C·ℓ/w` — an `−ω²`
//!   term, i.e. it folds into the **mass**:
//!
//!   ```text
//!   M_port = ((C̃ · ℓ) / w) · S_Γ.
//!   ```
//!
//! Here `ℓ` (gap length along `ê`) and `w` (width) are the uniform-port
//! geometry factors [`crate::mesh::transmon::TransmonPort`] recovers from
//! the tagged junction triangles. The PEC-bounded transmon eigenproblem
//! therefore stays a REAL symmetric generalized pencil
//!
//! ```text
//! (K + K_port) x = ω² (M + M_port) x,
//! ```
//!
//! with `K_port`, `M_port` PSD (positive scalars times the PSD `S_Γ`) —
//! solvable **as-is** with the real shift-invert Lanczos
//! ([`crate::eigen::lanczos::SparseShiftInvertLanczos`]). This matches
//! Palace's `LumpedPort`/`LumpedElement` with `L`/`C` in the eigenmode
//! problem type: purely reactive elements participate in the (real)
//! eigenproblem; only `R` makes it complex.
//!
//! # Natural-unit conversion
//!
//! The repo works in natural units (η₀ = μ₀ = ε₀ = 1); the mesh
//! coordinates are in the fixture's length unit (the DeviceLayout
//! transmon mesh is in **micrometres**). The SI element values convert to
//! natural-unit lengths via
//!
//! ```text
//! L̃ = L / μ₀,   C̃ = C / ε₀,
//! ```
//!
//! then are **rescaled into the mesh length unit** so `ℓ`, `w`, `L̃`, `C̃`
//! share one unit. See [`ReactiveElementNatural::from_si`]. Eigenvalues
//! come out as `λ = k²` in `(1/length-unit)²`; restore the physical
//! frequency with [`frequency_hz_from_lambda`].
//!
//! # Scope fences (v1)
//!
//! - **Resistive R in eigenmode is OUT of scope.** `R` makes the pencil
//!   complex (lossy modes, finite Q / κ). v1 drops the 50 Ω terms on
//!   `port_1`/`port_2` entirely (lossless approximation). The readout
//!   ports are left as natural (open) boundaries.
//! - **Full EPR post-processing is Phase C.** The only participation
//!   quantity here is the minimal junction-energy ratio
//!   [`ModeReport::participation`] used for mode identification.

use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::nedelec::NedelecScatterMap;
use crate::driven::ports::assemble_port_surface_mass;
use crate::eigen::dense::EigenError;
use crate::eigen::gauge::TreeCotreeGauge;
use crate::eigen::lanczos::SparseShiftInvertLanczos;
use crate::mesh::TetMesh;

/// Vacuum permeability `μ₀` (H/m), SI.
pub const MU0_SI: f64 = 4.0 * std::f64::consts::PI * 1e-7;
/// Vacuum permittivity `ε₀` (F/m), SI (CODATA).
pub const EPS0_SI: f64 = 8.8541878128e-12;
/// Speed of light `c` (m/s), SI (exact).
pub const C_LIGHT_SI: f64 = 299_792_458.0;

/// A reactive lumped element (`L` in parallel with `C`) in **natural
/// units rescaled to the mesh length unit**.
///
/// Both `l_natural` and `c_natural` carry the mesh length unit (e.g.
/// micrometres) so they compose directly with the port geometry factors
/// `ℓ`, `w` (also in mesh units) when forming `K_port`/`M_port`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactiveElementNatural {
    /// Inductance in natural units, rescaled to the mesh length unit:
    /// `L̃ = (L / μ₀) / m_per_unit`.
    pub l_natural: f64,
    /// Capacitance in natural units, rescaled to the mesh length unit:
    /// `C̃ = (C / ε₀) / m_per_unit`.
    pub c_natural: f64,
}

impl ReactiveElementNatural {
    /// Convert SI element values `(L [H], C [F])` to natural units in the
    /// mesh length unit.
    ///
    /// `m_per_unit` is the length of one mesh unit in metres (e.g.
    /// `1e-6` for a micrometre-unit mesh). The natural-unit conversion
    /// `L̃ = L/μ₀`, `C̃ = C/ε₀` yields lengths in **metres**; dividing by
    /// `m_per_unit` re-expresses them in the mesh unit so they compose
    /// with `ℓ`, `w`.
    ///
    /// # Panics
    ///
    /// Panics if `m_per_unit` is not strictly positive.
    pub fn from_si(l_henry: f64, c_farad: f64, m_per_unit: f64) -> Self {
        assert!(
            m_per_unit > 0.0,
            "m_per_unit must be positive, got {m_per_unit}"
        );
        Self {
            l_natural: (l_henry / MU0_SI) / m_per_unit,
            c_natural: (c_farad / EPS0_SI) / m_per_unit,
        }
    }
}

/// A reactive lumped-shunt surface term on a tagged port patch: produces
/// the `K_port` and `M_port` scalings of the real symmetric surface mass
/// `S_Γ` per the module derivation.
///
/// The surface mass itself is assembled by the shared Whitney kernel
/// ([`assemble_port_surface_mass`]) — the same bit-identical-triplet
/// machinery the driven port uses. This struct only carries the geometry
/// and element values and applies the scalar scalings.
#[derive(Debug, Clone)]
pub struct LumpedReactiveShunt<'a> {
    /// Port surface triangles (0-based node triples into `mesh.nodes`).
    pub faces: &'a [[u32; 3]],
    /// Gap length `ℓ` along `ê`, in mesh units.
    pub length: f64,
    /// Port width `w` (area / length), in mesh units.
    pub width: f64,
    /// Reactive element values in natural units (mesh length unit).
    pub element: ReactiveElementNatural,
}

impl LumpedReactiveShunt<'_> {
    /// Stiffness scaling `ℓ / (w · L̃)` applied to `S_Γ` to form `K_port`.
    /// Zero when `l_natural` is infinite (junction removed) — see
    /// [`LumpedReactiveShunt::without_inductor`].
    pub fn k_scale(&self) -> f64 {
        if self.element.l_natural.is_infinite() {
            0.0
        } else {
            self.length / (self.width * self.element.l_natural)
        }
    }

    /// Mass scaling `(C̃ · ℓ) / w` applied to `S_Γ` to form `M_port`.
    pub fn m_scale(&self) -> f64 {
        self.element.c_natural * self.length / self.width
    }

    /// A copy of this shunt with the inductor removed (`L̃ = ∞`, so
    /// `k_scale() == 0`) — the junction-removal tripwire. The capacitance
    /// is retained (the `M_port` choice is stated explicitly).
    pub fn without_inductor(&self) -> Self {
        let mut out = self.clone();
        out.element.l_natural = f64::INFINITY;
        out
    }

    /// Assemble the `K_port` surface triplets `(row, col, value)` over
    /// global edge indices: `k_scale · S_Γ`. Empty when `k_scale() == 0`.
    pub fn k_port_triplets(&self, mesh: &TetMesh, edges: &[[u32; 2]]) -> Vec<(usize, usize, f64)> {
        let s = self.k_scale();
        if s == 0.0 {
            return Vec::new();
        }
        assemble_port_surface_mass(mesh, self.faces, edges)
            .into_iter()
            .map(|(r, c, v)| (r, c, s * v))
            .collect()
    }

    /// Assemble the `M_port` surface triplets `(row, col, value)` over
    /// global edge indices: `m_scale · S_Γ`.
    pub fn m_port_triplets(&self, mesh: &TetMesh, edges: &[[u32; 2]]) -> Vec<(usize, usize, f64)> {
        let s = self.m_scale();
        assemble_port_surface_mass(mesh, self.faces, edges)
            .into_iter()
            .map(|(r, c, v)| (r, c, s * v))
            .collect()
    }
}

/// Restore a physical frequency (Hz) from an eigenvalue `λ = k²`
/// (in `(1/mesh-unit)²`).
///
/// `k = √λ` is a wavenumber in `1/mesh-unit`; converting to `1/m` via
/// `m_per_unit` and using `f = c·k / (2π)`:
///
/// ```text
/// f = c · √λ / (m_per_unit · 2π).
/// ```
///
/// Negative `λ` (gradient-nullspace rounding noise) yields `0` rather
/// than a NaN.
pub fn frequency_hz_from_lambda(lambda: f64, m_per_unit: f64) -> f64 {
    if lambda <= 0.0 {
        return 0.0;
    }
    C_LIGHT_SI * lambda.sqrt() / (m_per_unit * 2.0 * std::f64::consts::PI)
}

/// The eigenvalue shift `σ = k²` targeting a given physical frequency
/// (Hz) on a mesh with `m_per_unit` metres per unit — the inverse of
/// [`frequency_hz_from_lambda`].
pub fn lambda_shift_for_frequency_hz(f_hz: f64, m_per_unit: f64) -> f64 {
    let k = 2.0 * std::f64::consts::PI * f_hz * m_per_unit / C_LIGHT_SI;
    k * k
}

/// A single computed eigenmode with its junction participation and
/// restored physical frequency.
#[derive(Debug, Clone)]
pub struct ModeReport {
    /// Eigenvalue `λ = k²` in `(1/mesh-unit)²`.
    pub lambda: f64,
    /// Restored physical frequency (Hz).
    pub frequency_hz: f64,
    /// Junction participation `p = (xᵀ K_port x) / (xᵀ (K + K_port) x)
    /// ∈ [0, 1]` — the minimal EPR precursor for mode identification. The
    /// qubit-like mode has large `p`, the resonator-like mode small `p`.
    pub participation: f64,
}

impl ModeReport {
    /// Frequency in GHz (convenience for reporting against the blog band).
    pub fn frequency_ghz(&self) -> f64 {
        self.frequency_hz / 1e9
    }
}

/// Build a real faer [`SparseColMat`] from a `[nnz]` value slice aligned
/// to the volume sparsity pattern of `scatter`, restricted to the
/// interior DOFs, with **extra surface triplets** added on top.
///
/// `interior_index[e]` is `Some(reduced_index)` for a kept interior edge
/// and `None` for a PEC (Dirichlet) edge. Volume entries whose row or
/// column is Dirichlet are dropped (the standard interior reduction). The
/// `extra` triplets (K_port/M_port over global edge indices) are reindexed
/// the same way and summed by faer's `try_new_from_triplets`.
fn assemble_reduced_real(
    pattern_rows: &[u32],
    pattern_cols: &[u32],
    vals: &[f64],
    extra: &[(usize, usize, f64)],
    interior_index: &[Option<usize>],
    dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(vals.len() + extra.len());
    for ((&r, &c), &v) in pattern_rows
        .iter()
        .zip(pattern_cols.iter())
        .zip(vals.iter())
    {
        if let (Some(ri), Some(ci)) = (interior_index[r as usize], interior_index[c as usize]) {
            trips.push(Triplet::new(ri, ci, v));
        }
    }
    for &(r, c, v) in extra {
        if let (Some(ri), Some(ci)) = (interior_index[r], interior_index[c]) {
            trips.push(Triplet::new(ri, ci, v));
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("reduced sparse assembly: {e:?}")))
}

/// Inputs to [`solve_transmon_eigenmodes`]: the assembled real pencil
/// value vectors plus the junction shunt and reduction bookkeeping.
///
/// The value vectors `k_vals`/`m_vals` are the **real parts** of the
/// sparse full-tensor Nédélec assembly
/// ([`crate::assembly::nedelec::assemble_global_nedelec_with_full_tensors_sparse`]),
/// pulled to host `f64` and aligned to `scatter.pattern()`. The imaginary
/// parts are asserted ~0 by the caller (the sapphire ε tensor is lossless
/// here, so the pencil is exactly real).
pub struct TransmonPencil<'a> {
    /// Volume sparsity-pattern + slot map the value vectors align to.
    pub scatter: &'a NedelecScatterMap,
    /// Curl-curl stiffness values `[nnz]` in pattern order (real part).
    pub k_vals: &'a [f64],
    /// ε-weighted mass values `[nnz]` in pattern order (real part).
    pub m_vals: &'a [f64],
    /// Global edge table (`mesh.edges()`), for the surface-mass assembly.
    pub edges: &'a [[u32; 2]],
    /// The mesh (for the Whitney surface kernel).
    pub mesh: &'a TetMesh,
    /// The junction reactive shunt on the `lumped_element` patch.
    pub shunt: LumpedReactiveShunt<'a>,
    /// Interior-DOF mask over `edges` (`true` = kept interior edge).
    pub interior_mask: &'a [bool],
}

/// Solve the transmon eigenmodes: assemble the real pencil
/// `(K + K_port) x = λ (M + M_port) x`, reduce over the interior DOFs,
/// and run shift-invert Lanczos near `sigma` for `n_modes` modes. Returns
/// the modes with restored physical frequencies and junction
/// participation, plus the reduced `(K + K_port)` and `K_port` matrices
/// (retained for tripwire / diagnostic re-use).
///
/// `sigma` is the eigenvalue shift `λ = k²` (use
/// [`lambda_shift_for_frequency_hz`] to place it in the expected band and
/// separate the modes from the gradient nullspace at 0).
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly or the Lanczos
/// solve.
pub fn solve_transmon_eigenmodes(
    pencil: &TransmonPencil<'_>,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<Vec<ModeReport>, EigenError> {
    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );

    // Ungauged path: reindex is the plain PEC interior reduction.
    let mut interior_index = vec![None; n_edges];
    let mut dim = 0usize;
    for (e, &keep) in pencil.interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no interior DOFs after PEC reduction".into(),
        ));
    }
    solve_transmon_eigenmodes_reindexed(pencil, &interior_index, dim, sigma, n_modes, m_per_unit)
}

/// Tree-cotree **gauged** transmon eigensolve (issue #502).
///
/// Identical to [`solve_transmon_eigenmodes`] except that the reduced
/// pencil is additionally restricted to the **cotree** edges of a spanning
/// tree of the mesh node graph ([`TreeCotreeGauge`]), eliminating exactly
/// `rank(d⁰_interior)` gradient DOFs (`kernel(K) = image(d⁰)`). The gauged
/// pencil is smaller (`interior_dim − tree_edges` DOFs), which speeds the
/// sparse LU up.
///
/// # NOT spectrum-preserving for the eigenproblem
///
/// DOF elimination (drop tree rows/cols of BOTH `K` and `M`) is the correct
/// gauge for the curl-curl *source* problem but **shifts** the generalized
/// eigenproblem's physical spectrum, because the physical eigenvectors have
/// nonzero tree-edge components (see [`TreeCotreeGauge`] docs and the
/// measured 1.64% resonator drift in
/// `tests/transmon_eigenmode.rs::tree_cotree_dof_elimination_shifts_eigen_spectrum`).
/// This entry point exists to exercise and pin that finding; the
/// spectrum-preserving fix is a divergence-free projection (issue #502
/// follow-on). Do **not** use it as the physical transmon solver — the
/// ungauged [`solve_transmon_eigenmodes`] remains the committed benchmark
/// path.
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly or the Lanczos
/// solve; errors if the gauge leaves no cotree DOFs.
pub fn solve_transmon_eigenmodes_gauged(
    pencil: &TransmonPencil<'_>,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<Vec<ModeReport>, EigenError> {
    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );
    let gauge = TreeCotreeGauge::build(pencil.edges, pencil.interior_mask, pencil.mesh.n_nodes());
    let dim = gauge.gauged_dim();
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no cotree DOFs after tree-cotree gauge".into(),
        ));
    }
    solve_transmon_eigenmodes_reindexed(
        pencil,
        gauge.gauged_index_map(),
        dim,
        sigma,
        n_modes,
        m_per_unit,
    )
}

/// Shared core: solve the reduced real pencil under an arbitrary
/// global-edge → reduced-DOF `reindex` (`Some(r)` = kept DOF at reduced
/// index `r`, `None` = eliminated), with reduced dimension `dim`. The
/// ungauged path passes the plain PEC interior reindex; the gauged path
/// passes the tree-cotree cotree reindex. Everything downstream (surface
/// terms, reduced assembly, Lanczos, participation) is index-agnostic.
fn solve_transmon_eigenmodes_reindexed(
    pencil: &TransmonPencil<'_>,
    reindex: &[Option<usize>],
    dim: usize,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<Vec<ModeReport>, EigenError> {
    let pattern = pencil.scatter.pattern();
    assert_eq!(pencil.k_vals.len(), pattern.nnz(), "k_vals length mismatch");
    assert_eq!(pencil.m_vals.len(), pattern.nnz(), "m_vals length mismatch");
    let interior_index = reindex;

    // Junction surface terms over the full edge set.
    let k_port = pencil.shunt.k_port_triplets(pencil.mesh, pencil.edges);
    let m_port = pencil.shunt.m_port_triplets(pencil.mesh, pencil.edges);

    // Reduced (K + K_port) and (M + M_port) real sparse matrices.
    let k_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.k_vals,
        &k_port,
        interior_index,
        dim,
    )?;
    let m_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.m_vals,
        &m_port,
        interior_index,
        dim,
    )?;

    // K_port alone (reduced) — needed for the participation numerator.
    let k_port_red = assemble_reduced_real(&[], &[], &[], &k_port, interior_index, dim)?;

    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-8,
    };
    let pairs = solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), n_modes)?;

    Ok(pairs
        .iter()
        .map(|pair| ModeReport {
            lambda: pair.lambda,
            frequency_hz: frequency_hz_from_lambda(pair.lambda, m_per_unit),
            participation: junction_participation(&k_red, &k_port_red, &pair.vector),
        })
        .collect())
}

/// Junction participation `p = (xᵀ K_port x) / (xᵀ (K + K_port) x)`.
///
/// Both quadratic forms are evaluated on the reduced interior-DOF vector.
/// Clamped to `[0, 1]` against rounding noise. Returns `0` if the
/// denominator is non-positive (degenerate).
fn junction_participation(
    k_total: &SparseColMat<usize, f64>,
    k_port: &SparseColMat<usize, f64>,
    x: &[f64],
) -> f64 {
    let num = quad_form(k_port, x);
    let den = quad_form(k_total, x);
    if den <= 0.0 {
        return 0.0;
    }
    (num / den).clamp(0.0, 1.0)
}

/// Quadratic form `xᵀ A x` for a CSC sparse matrix.
fn quad_form(a: &SparseColMat<usize, f64>, x: &[f64]) -> f64 {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let mut acc = 0.0;
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            acc += x[row_idx[k]] * val[k] * xj;
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{TetMesh, cube_tet_mesh};

    /// Hand-computed natural-unit conversion for the DeviceLayout junction
    /// values on a micrometre-unit mesh (issue #492 derivation):
    /// `L̃ = 14.860 nH / μ₀ ≈ 1.18253×10⁴ μm`,
    /// `C̃ = 5.5 fF / ε₀ ≈ 621.17 μm`.
    #[test]
    fn si_to_natural_matches_hand_numbers() {
        let e = ReactiveElementNatural::from_si(14.860e-9, 5.5e-15, 1e-6);
        // L̃ in μm.
        let l_expected = (14.860e-9 / MU0_SI) / 1e-6; // ≈ 1.18253e4
        let c_expected = (5.5e-15 / EPS0_SI) / 1e-6; // ≈ 621.17
        assert!(
            (e.l_natural - l_expected).abs() / l_expected < 1e-12,
            "L̃ = {}, want {l_expected}",
            e.l_natural
        );
        assert!(
            (e.c_natural - c_expected).abs() / c_expected < 1e-12,
            "C̃ = {}, want {c_expected}",
            e.c_natural
        );
        // Order-of-magnitude anchors from the issue body.
        assert!(
            (e.l_natural - 1.18253e4).abs() < 5.0,
            "L̃ ≈ 1.18253e4 μm, got {}",
            e.l_natural
        );
        assert!(
            (e.c_natural - 621.17).abs() < 0.5,
            "C̃ ≈ 621.17 μm, got {}",
            e.c_natural
        );
    }

    /// A flat rectangular port patch on `z = 0` of the unit cube.
    fn z0_port(mesh: &TetMesh) -> Vec<[u32; 3]> {
        mesh.faces()
            .into_iter()
            .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
            .collect()
    }

    fn shunt_on<'a>(faces: &'a [[u32; 3]], l: f64, c: f64) -> LumpedReactiveShunt<'a> {
        LumpedReactiveShunt {
            faces,
            length: 1.0,
            width: 1.0,
            element: ReactiveElementNatural {
                l_natural: l,
                c_natural: c,
            },
        }
    }

    /// K_port must be symmetric (it inherits S_Γ's symmetry) and PSD
    /// (`xᵀ K_port x ≥ 0` for random x, since `k_scale ≥ 0` and S_Γ is
    /// PSD).
    #[test]
    fn k_port_symmetric_and_psd() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces = z0_port(&mesh);
        let shunt = shunt_on(&faces, 10.0, 5.0);
        let n = edges.len();

        let mut dense = vec![0.0_f64; n * n];
        for (r, c, v) in shunt.k_port_triplets(&mesh, &edges) {
            dense[r * n + c] += v;
        }
        // Symmetry.
        let mut max_asym = 0.0_f64;
        for r in 0..n {
            for c in 0..n {
                max_asym = max_asym.max((dense[r * n + c] - dense[c * n + r]).abs());
            }
        }
        assert!(max_asym < 1e-14, "K_port not symmetric: {max_asym}");
        // PSD: xᵀ K_port x ≥ 0 on several deterministic random vectors.
        for seed in 0..8u64 {
            let x: Vec<f64> = (0..n)
                .map(|i| (((i as u64 + seed * 7 + 1) as f64) * 0.7391).sin())
                .collect();
            let mut q = 0.0;
            for r in 0..n {
                for c in 0..n {
                    q += x[r] * dense[r * n + c] * x[c];
                }
            }
            assert!(q >= -1e-12, "K_port not PSD: xᵀKx = {q}");
        }
    }

    /// Doubling `L̃` halves every `K_port` entry; doubling `C̃` doubles
    /// every `M_port` entry (linear scaling of the surface term).
    #[test]
    fn linear_scaling_in_l_and_c() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces = z0_port(&mesh);

        let base = shunt_on(&faces, 10.0, 5.0);
        let double_l = shunt_on(&faces, 20.0, 5.0);
        let double_c = shunt_on(&faces, 10.0, 10.0);

        let k_base = base.k_port_triplets(&mesh, &edges);
        let k_dl = double_l.k_port_triplets(&mesh, &edges);
        assert_eq!(k_base.len(), k_dl.len());
        for (b, d) in k_base.iter().zip(k_dl.iter()) {
            assert_eq!(b.0, d.0);
            assert_eq!(b.1, d.1);
            assert!(
                (b.2 - 2.0 * d.2).abs() < 1e-12 * b.2.abs().max(1.0),
                "doubling L̃ must halve K_port: {} vs {}",
                b.2,
                d.2
            );
        }

        let m_base = base.m_port_triplets(&mesh, &edges);
        let m_dc = double_c.m_port_triplets(&mesh, &edges);
        assert_eq!(m_base.len(), m_dc.len());
        for (b, d) in m_base.iter().zip(m_dc.iter()) {
            assert!(
                (2.0 * b.2 - d.2).abs() < 1e-12 * d.2.abs().max(1.0),
                "doubling C̃ must double M_port: {} vs {}",
                b.2,
                d.2
            );
        }
    }

    /// The scalings compose the geometry factors exactly: `k_scale =
    /// ℓ/(w·L̃)` and `m_scale = C̃·ℓ/w` for hand-picked `ℓ`, `w`.
    #[test]
    fn scale_factors_match_geometry() {
        let faces: Vec<[u32; 3]> = Vec::new();
        let shunt = LumpedReactiveShunt {
            faces: &faces,
            length: 3.0,
            width: 2.0,
            element: ReactiveElementNatural {
                l_natural: 5.0,
                c_natural: 7.0,
            },
        };
        assert!((shunt.k_scale() - 3.0 / (2.0 * 5.0)).abs() < 1e-15);
        assert!((shunt.m_scale() - 7.0 * 3.0 / 2.0).abs() < 1e-15);
    }

    /// Junction removal (`without_inductor`) zeroes `k_scale` (so
    /// `K_port` is empty) but preserves the capacitive `M_port`.
    #[test]
    fn junction_removal_zeroes_k_keeps_m() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces = z0_port(&mesh);
        let shunt = shunt_on(&faces, 10.0, 5.0);
        let removed = shunt.without_inductor();

        assert_eq!(removed.k_scale(), 0.0);
        assert!(removed.k_port_triplets(&mesh, &edges).is_empty());
        // M_port unchanged.
        assert_eq!(
            removed.m_port_triplets(&mesh, &edges).len(),
            shunt.m_port_triplets(&mesh, &edges).len()
        );
    }

    /// Uniform-field closed form: for the constant tangential field
    /// `E = ê` interpolated on a flat port, `xᵀ S_Γ x = Area` (the
    /// Whitney elements reproduce constants exactly). Here the z=0 face of
    /// the unit cube has area 1, so with `ê = x̂` the quadratic form of
    /// the surface mass equals 1.
    #[test]
    fn uniform_field_surface_mass_closed_form() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces = z0_port(&mesh);
        // Edge DOFs of the constant field E = x̂: e_i = x_b − x_a.
        let x: Vec<f64> = edges
            .iter()
            .map(|e| mesh.nodes[e[1] as usize][0] - mesh.nodes[e[0] as usize][0])
            .collect();
        let n = edges.len();
        let mut s = vec![0.0_f64; n * n];
        for (r, c, v) in assemble_port_surface_mass(&mesh, &faces, &edges) {
            s[r * n + c] += v;
        }
        let mut q = 0.0;
        for r in 0..n {
            for c in 0..n {
                q += x[r] * s[r * n + c] * x[c];
            }
        }
        assert!(
            (q - 1.0).abs() < 1e-12,
            "uniform-field xᵀ S_Γ x = {q}, want Area = 1"
        );
    }

    /// `frequency_hz_from_lambda` and `lambda_shift_for_frequency_hz` are
    /// inverses, and a 5 GHz mode on a μm mesh lands at `λ ≈ 1.1e-8 μm⁻²`
    /// (the issue's shift-placement estimate).
    #[test]
    fn frequency_lambda_roundtrip() {
        let m_per_unit = 1e-6;
        let f = 5.0e9;
        let lambda = lambda_shift_for_frequency_hz(f, m_per_unit);
        assert!(
            (lambda - 1.098e-8).abs() < 0.05e-8,
            "λ(5 GHz, μm) ≈ 1.1e-8, got {lambda}"
        );
        let f_back = frequency_hz_from_lambda(lambda, m_per_unit);
        assert!(
            (f_back - f).abs() / f < 1e-12,
            "roundtrip f = {f_back}, want {f}"
        );
        // Non-positive λ → 0 Hz (nullspace guard).
        assert_eq!(frequency_hz_from_lambda(-1.0, m_per_unit), 0.0);
    }
}
