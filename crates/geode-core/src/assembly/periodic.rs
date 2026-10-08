//! Periodic DOF constraint: the prolongation `P` of a periodic boundary
//! condition and the operator reduction `A_r = Pᴴ A P` (issue #839, Epic
//! #837 Phase 1).
//!
//! A [`PeriodicConstraint`] is built from a [`PeriodicMap`] (the mesh-level
//! node / edge / face pairing) on an [`HcurlSpace`], plus an optional PEC
//! mask. Every **full** DOF gets a row of the sparse prolongation `P`
//! (`n_full × n_reduced`): an eliminated DOF has an empty row, every other
//! DOF has one entry `coeff` in the column of its class's **reduced** DOF.
//! At zero phase (`k = 0`, this phase) `coeff = σ ∈ {±1}`, the edge's
//! relative orientation; the Bloch phase of Epic #837 Phase 2 multiplies it
//! by `e^{−j k·Σd}` with `Σd` the alias's lattice vector
//! ([`DofAliasMap::lattice_shift`], [`DofAliasMap::dphase_dk`]). The
//! coefficient type is therefore `c64` from the start.
//!
//! # Reduction
//!
//! Because a row of `P` holds at most one entry (block size 1), `Pᴴ A P`
//! is an **index remap with scaling** over the existing sparsity pattern —
//! each stored `a_ij` lands at `(r(i), r(j))` scaled by `conj(c_i)·c_j`,
//! duplicates summed — not a general sparse product
//! ([`DofAliasMap::reduce_matrix`]). Vectors reduce as `Pᴴ b`
//! ([`DofAliasMap::reduce_vector`]) and solutions expand as `P x_r`
//! ([`DofAliasMap::expand`]).
//!
//! # Block size (the p=2 hook)
//!
//! The row storage is CSR, so a row may hold **several** entries: the p=2
//! sibling epic (#836) stores a p=2 face pair `(φ0, φ1)` as a dense 2×2
//! block from [`HcurlSpace::face_dof_transform`] (two entries per slave
//! face row), and a p=2 edge pair `(W, Q)` as a 2-diagonal block (`W` takes
//! `σ`, `Q` is orientation-free). The reduction loops above are already
//! written for any number of entries per row. This phase implements block
//! size 1 (p=1 Whitney edges, P1 nodes) only: a p=2 space returns
//! [`PeriodicError::Unsupported`].
//!
//! # PEC composition
//!
//! A reduced DOF is PEC if **any** of its aliases is. If a PEC wall covers a
//! master but not its slave (a geometry inconsistent with periodicity, e.g.
//! a strip that touches only one side), both copies are eliminated —
//! periodicity already forces the field to vanish there — and the class is
//! counted in [`PeriodicConstraintReport::n_one_sided_pec`] with a warning
//! ([`PeriodicConstraint::warnings`]). It is not an error.
//!
//! # Nodes and the discrete gradient
//!
//! The same alias logic on P1 nodes ([`PeriodicConstraint::nodes`]) is what
//! the gauge / gradient-nullspace machinery needs: the reduced discrete
//! gradient `G_r` ([`PeriodicConstraint::reduced_gradient`]) satisfies
//! `G P_node = P_edge G_r` exactly, which
//! [`PeriodicConstraint::check_gradient_commutes`] verifies rather than
//! assumes. A node is PEC when it is an endpoint of a PEC edge (the
//! face-exact masks of #771 eliminate exactly the edges of wall
//! triangles).
//!
//! # Design rule
//!
//! The constraint inherits the matching rule of [`crate::mesh::periodic`]:
//! warn and snap where a topological match exists, fail loudly where none
//! does. It never couples non-matching DOFs.

use std::ops::{Add, AddAssign};

use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::assembly::hcurl_space::HcurlSpace;
use crate::elements::ElementOrder;
use crate::mesh::periodic::{PeriodicMap, PeriodicMatchError};

/// Errors of the periodic constraint and the periodic solves.
#[derive(Debug, thiserror::Error)]
pub enum PeriodicError {
    /// A feature is not supported with periodic boundaries yet. The CLI maps
    /// this to `invalid_spec`.
    #[error("{feature} is not supported with periodic boundaries yet ({phase})")]
    Unsupported {
        /// The unsupported feature.
        feature: &'static str,
        /// Where support is planned.
        phase: &'static str,
    },
    /// The map, space, mask or operator do not describe the same mesh.
    #[error("periodic constraint mismatch: {0}")]
    Mismatch(String),
    /// Mesh matching failed.
    #[error(transparent)]
    Match(#[from] PeriodicMatchError),
    /// A lumped port's edges touch a periodic face (its gap would be
    /// aliased onto the partner face).
    #[error(
        "lumped port {port} has edges on a periodic face; move the port off the periodic \
         boundary"
    )]
    PortOnPeriodicFace {
        /// The port index.
        port: usize,
    },
    /// A real (`f64`) reduction was requested with complex (Bloch-phase)
    /// coefficients.
    #[error("cannot reduce a real matrix with complex (Bloch-phase) periodic coefficients")]
    ComplexCoefficients,
    /// Sparse assembly / factorization / solve failure.
    #[error("periodic solve failed: {0}")]
    Solve(String),
    /// An underlying driven-operator error.
    #[error(transparent)]
    Driven(#[from] crate::driven::solve::DrivenError),
}

/// What a full DOF becomes under the constraint (block-size-1 rows).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum DofAlias {
    /// PEC-eliminated (forced to zero).
    Eliminated,
    /// `x_full = coeff · x_reduced[index]`.
    Reduced {
        /// Reduced DOF index.
        index: usize,
        /// Coefficient (`σ` at zero phase).
        coeff: c64,
    },
}

/// Scalar types [`DofAliasMap::reduce_matrix`] / [`DofAliasMap::expand`]
/// accept: `f64` (zero-phase, real `P` only) and `c64`.
pub trait PeriodicScalar:
    faer::traits::ComplexField + Copy + Add<Output = Self> + AddAssign
{
    /// Whether the type carries an imaginary part.
    const IS_COMPLEX: bool;
    /// Additive identity.
    fn zero_value() -> Self;
    /// `self · c` (the real part of `c` for `f64`; callers check
    /// [`DofAliasMap::is_real`] first).
    fn times(self, c: c64) -> Self;
}

impl PeriodicScalar for f64 {
    const IS_COMPLEX: bool = false;
    fn zero_value() -> Self {
        0.0
    }
    fn times(self, c: c64) -> Self {
        self * c.re
    }
}

impl PeriodicScalar for c64 {
    const IS_COMPLEX: bool = true;
    fn zero_value() -> Self {
        c64::new(0.0, 0.0)
    }
    fn times(self, c: c64) -> Self {
        self * c
    }
}

/// Round-off tolerance (rad, per rad of phase magnitude above 1) of
/// [`is_lattice_phase`]: the same `1e-14` as the phase-factor snap of
/// [`DofAliasMap::with_bloch_phase`], scaled with the magnitude
/// `Σ_i |k_i d_i|` because the round-off of `θ = k·Σd` grows with it
/// (issue #915).
pub const LATTICE_PHASE_TOL: f64 = 1e-14;

/// A Bloch phase `θ` (rad) wrapped to `[−π, π]` with the `f64` value of
/// `2π`: `θ − 2π·round(θ / 2π)`. A `k` written as an integer multiple of
/// `2π / L` therefore wraps to exactly `0`, at any multiple.
pub fn wrap_phase(theta: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    theta - two_pi * (theta / two_pi).round()
}

/// Whether a Bloch phase `θ = k·Σd` (rad) is in `2πℤ` up to round-off:
/// `|wrap_phase(θ)| ≤ LATTICE_PHASE_TOL · max(1, magnitude)`, with
/// `magnitude = Σ_i |k_i d_i|`. The magnitude, not `|θ|`, sets the
/// round-off: the terms of `k·Σd` can cancel (measured: `θ = 5.7e-14` for
/// `2π·64/L_x − 2π·64/L_y` on a diagonal shift). This is the
/// reciprocal-lattice test of [`DofAliasMap::with_bloch_phase`] (its
/// phase factor is then exactly `1`), so it agrees with a distance built
/// from [`wrap_phase`] at any `|G|` (issue #915).
pub fn is_lattice_phase(theta: f64, magnitude: f64) -> bool {
    wrap_phase(theta).abs() <= LATTICE_PHASE_TOL * magnitude.max(1.0)
}

/// A sparse prolongation `P` (`n_full × n_reduced`) stored by rows, with the
/// lattice vector of every row. Used for both the edge (or general H(curl)
/// DOF) and the node constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct DofAliasMap {
    n_reduced: usize,
    row_ptr: Vec<usize>,
    cols: Vec<usize>,
    coeffs: Vec<c64>,
    /// Zero-phase coefficients (`σ`, or the p=2 block entries): the
    /// Bloch phase multiplies these, so re-phasing is never cumulative.
    base: Vec<c64>,
    /// Lattice vector `Σd` of each full DOF from its canonical master.
    shift: Vec<[f64; 3]>,
    /// The Bloch wave vector the coefficients carry (zero at construction).
    bloch_k: [f64; 3],
}

impl DofAliasMap {
    /// Build from per-row entries.
    fn from_rows(n_reduced: usize, rows: Vec<Vec<(usize, c64)>>, shift: Vec<[f64; 3]>) -> Self {
        let mut row_ptr = Vec::with_capacity(rows.len() + 1);
        row_ptr.push(0);
        let mut cols = Vec::new();
        let mut coeffs = Vec::new();
        for r in rows {
            for (c, v) in r {
                cols.push(c);
                coeffs.push(v);
            }
            row_ptr.push(cols.len());
        }
        Self {
            n_reduced,
            row_ptr,
            cols,
            base: coeffs.clone(),
            coeffs,
            shift,
            bloch_k: [0.0; 3],
        }
    }

    /// The same prolongation with the **Bloch phase** of wave vector `k`
    /// (rad per mesh length unit): every stored coefficient becomes
    /// `c₀ · e^{−j k·Σd}`, with `c₀` the zero-phase coefficient and `Σd`
    /// the row's lattice vector (Epic #837 Phase 2, issue #858). The phase
    /// is applied to the zero-phase coefficients, so calling this on an
    /// already-phased map replaces its `k` (it does not compose). Phase
    /// factor components within `1e-14` of zero are snapped to zero, so
    /// `k·Σd ∈ πℤ` yields an exactly real (`±1`) `P`. A phase that
    /// [`is_lattice_phase`] (wrapped within round-off of `2πℤ`, at any
    /// magnitude) gives a factor of exactly `1` (issue #915).
    ///
    /// With a complex phase, `Pᴴ A P` is **Hermitian** for a real
    /// symmetric `A`, not complex-symmetric: `(Pᴴ A P)ᵀ = Pᵀ A P̄`, the
    /// reduction at `−k`.
    pub fn with_bloch_phase(&self, k: [f64; 3]) -> Self {
        let mut out = self.clone();
        for i in 0..self.n_full() {
            let d = self.shift[i];
            let phase = -(k[0] * d[0] + k[1] * d[1] + k[2] * d[2]);
            // Snap round-off so a phase in (π/2)ℤ (periodic, anti-periodic,
            // quarter-wave) is exact: e^{−jπ} is −1, not −1 − 1.2e-16 j.
            let snap = |x: f64| if x.abs() < 1e-14 { 0.0 } else { x };
            let magnitude = (k[0] * d[0]).abs() + (k[1] * d[1]).abs() + (k[2] * d[2]).abs();
            let ph = if is_lattice_phase(phase, magnitude) {
                // A reciprocal lattice vector at large |G|: sin(2π·64) is
                // ~4e-14, above the absolute snap (issue #915).
                c64::new(1.0, 0.0)
            } else {
                c64::new(snap(phase.cos()), snap(phase.sin()))
            };
            for e in self.row_ptr[i]..self.row_ptr[i + 1] {
                out.coeffs[e] = if d == [0.0; 3] {
                    self.base[e]
                } else {
                    self.base[e] * ph
                };
            }
        }
        out.bloch_k = k;
        out
    }

    /// The Bloch wave vector the coefficients carry (`[0; 3]` unless set by
    /// [`Self::with_bloch_phase`]).
    pub fn bloch_k(&self) -> [f64; 3] {
        self.bloch_k
    }

    /// Full DOF count (rows of `P`).
    pub fn n_full(&self) -> usize {
        self.row_ptr.len() - 1
    }

    /// Reduced DOF count (columns of `P`).
    pub fn n_reduced(&self) -> usize {
        self.n_reduced
    }

    /// Row `i` of `P` as `(reduced index, coefficient)` pairs: empty for an
    /// eliminated DOF, one entry for block size 1 (p=1), up to two for the
    /// p=2 blocks.
    pub fn row(&self, i: usize) -> impl Iterator<Item = (usize, c64)> + '_ {
        let (a, b) = (self.row_ptr[i], self.row_ptr[i + 1]);
        self.cols[a..b]
            .iter()
            .copied()
            .zip(self.coeffs[a..b].iter().copied())
    }

    /// Whether `self` and `other` are the same prolongation `P`: equal
    /// shape and, row by row, equal `(reduced index, coefficient)` entries.
    /// The Bloch wave vector they were phased with is not compared, so
    /// `P(G)` equals `P(0)` for a reciprocal lattice vector `G`. Valid for
    /// p=2 block rows, unlike a comparison through [`Self::alias`] (issue
    /// #915).
    pub fn same_prolongation(&self, other: &Self) -> bool {
        self.n_reduced == other.n_reduced
            && self.n_full() == other.n_full()
            && (0..self.n_full()).all(|i| self.row(i).eq(other.row(i)))
    }

    /// The alias of full DOF `i`.
    ///
    /// # Panics
    ///
    /// Panics if row `i` has more than one entry (a p=2 block row; use
    /// [`Self::row`]).
    pub fn alias(&self, i: usize) -> DofAlias {
        let (a, b) = (self.row_ptr[i], self.row_ptr[i + 1]);
        match b - a {
            0 => DofAlias::Eliminated,
            1 => DofAlias::Reduced {
                index: self.cols[a],
                coeff: self.coeffs[a],
            },
            _ => panic!("DOF {i} is a block row; use DofAliasMap::row"),
        }
    }

    /// Lattice vector `Σd` of full DOF `i` from its canonical master (zero
    /// off the periodic faces and for the masters).
    pub fn lattice_shift(&self, i: usize) -> [f64; 3] {
        self.shift[i]
    }

    /// Whether every coefficient is real (always at zero phase).
    pub fn is_real(&self) -> bool {
        self.coeffs.iter().all(|c| c.im == 0.0)
    }

    /// `∂c/∂k` of every stored coefficient of `P(k)` under the Bloch phase
    /// `c(k) = c₀ e^{−j k·Σd}`: `∂c/∂k = −j Σd c`, one `[∂/∂k_x, ∂/∂k_y,
    /// ∂/∂k_z]` per stored entry, in row order (aligned with
    /// [`Self::row`]).
    ///
    /// The hook for group velocity / Hellmann–Feynman (Epic #837 Phase 2,
    /// #841): `∂A_r/∂k = (∂P/∂k)ᴴ A P + Pᴴ A (∂P/∂k)`. At zero phase it is
    /// already exact (the derivative at `k = 0`).
    pub fn dphase_dk(&self) -> Vec<[c64; 3]> {
        let mut out = Vec::with_capacity(self.coeffs.len());
        for i in 0..self.n_full() {
            let d = self.shift[i];
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                let c = self.coeffs[k];
                let mj = c64::new(0.0, -1.0) * c;
                out.push([mj * d[0], mj * d[1], mj * d[2]]);
            }
        }
        out
    }

    /// `A_r = Pᴴ A P` for a full-DOF square matrix `A`, as an index remap
    /// with scaling over `A`'s pattern (duplicates summed).
    ///
    /// # Errors
    ///
    /// [`PeriodicError::Mismatch`] if `A` is not `n_full × n_full`;
    /// [`PeriodicError::ComplexCoefficients`] for an `f64` matrix and a
    /// complex `P`; [`PeriodicError::Solve`] if the sparse build fails.
    pub fn reduce_matrix<T: PeriodicScalar>(
        &self,
        a: SparseColMatRef<'_, usize, T>,
    ) -> Result<SparseColMat<usize, T>, PeriodicError> {
        let n = self.n_full();
        if a.nrows() != n || a.ncols() != n {
            return Err(PeriodicError::Mismatch(format!(
                "matrix is {}×{}, the constraint has {n} full DOFs",
                a.nrows(),
                a.ncols()
            )));
        }
        if !T::IS_COMPLEX && !self.is_real() {
            return Err(PeriodicError::ComplexCoefficients);
        }
        let mut tr: Vec<Triplet<usize, usize, T>> = Vec::with_capacity(a.compute_nnz());
        for j in 0..n {
            let (cj0, cj1) = (self.row_ptr[j], self.row_ptr[j + 1]);
            if cj0 == cj1 {
                continue;
            }
            for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
                for ki in self.row_ptr[i]..self.row_ptr[i + 1] {
                    let ci = self.coeffs[ki].conj();
                    for kj in cj0..cj1 {
                        let w = ci * self.coeffs[kj];
                        tr.push(Triplet::new(self.cols[ki], self.cols[kj], v.times(w)));
                    }
                }
            }
        }
        SparseColMat::try_new_from_triplets(self.n_reduced, self.n_reduced, &tr)
            .map_err(|e| PeriodicError::Solve(format!("reduced matrix assembly: {e:?}")))
    }

    /// `A_r = Pᴴ A P` for a **real** full-DOF matrix `A` and any (real or
    /// complex) `P`, returned complex. The Bloch-phase eigen path (issue
    /// #858) reduces the real lossless pencil this way at every `k` without
    /// keeping a complex copy of the full matrices.
    ///
    /// # Errors
    ///
    /// As [`Self::reduce_matrix`] (never
    /// [`PeriodicError::ComplexCoefficients`]).
    pub fn reduce_real_matrix(
        &self,
        a: SparseColMatRef<'_, usize, f64>,
    ) -> Result<SparseColMat<usize, c64>, PeriodicError> {
        let n = self.n_full();
        if a.nrows() != n || a.ncols() != n {
            return Err(PeriodicError::Mismatch(format!(
                "matrix is {}×{}, the constraint has {n} full DOFs",
                a.nrows(),
                a.ncols()
            )));
        }
        let mut tr: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(a.compute_nnz());
        for j in 0..n {
            let (cj0, cj1) = (self.row_ptr[j], self.row_ptr[j + 1]);
            if cj0 == cj1 {
                continue;
            }
            for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
                for ki in self.row_ptr[i]..self.row_ptr[i + 1] {
                    let ci = self.coeffs[ki].conj();
                    for kj in cj0..cj1 {
                        let w = ci * self.coeffs[kj];
                        tr.push(Triplet::new(self.cols[ki], self.cols[kj], w * v));
                    }
                }
            }
        }
        SparseColMat::try_new_from_triplets(self.n_reduced, self.n_reduced, &tr)
            .map_err(|e| PeriodicError::Solve(format!("reduced matrix assembly: {e:?}")))
    }

    /// `b_r = Pᴴ b`.
    ///
    /// # Panics
    ///
    /// Panics if `b.len() != self.n_full()`, or for an `f64` vector with a
    /// complex `P`.
    pub fn reduce_vector<T: PeriodicScalar>(&self, b: &[T]) -> Vec<T> {
        assert_eq!(b.len(), self.n_full(), "vector length != n_full");
        assert!(
            T::IS_COMPLEX || self.is_real(),
            "complex P on a real vector"
        );
        let mut out = vec![T::zero_value(); self.n_reduced];
        for (i, &bi) in b.iter().enumerate() {
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                out[self.cols[k]] += bi.times(self.coeffs[k].conj());
            }
        }
        out
    }

    /// `x = P x_r` (zeros on eliminated DOFs).
    ///
    /// # Panics
    ///
    /// Panics if `x_r.len() != self.n_reduced()`, or for an `f64` vector
    /// with a complex `P`.
    pub fn expand<T: PeriodicScalar>(&self, x_r: &[T]) -> Vec<T> {
        assert_eq!(x_r.len(), self.n_reduced, "vector length != n_reduced");
        assert!(
            T::IS_COMPLEX || self.is_real(),
            "complex P on a real vector"
        );
        (0..self.n_full())
            .map(|i| {
                let mut acc = T::zero_value();
                for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                    acc += x_r[self.cols[k]].times(self.coeffs[k]);
                }
                acc
            })
            .collect()
    }

    /// The rows `rows` of `P` (a sub-prolongation over a subset of the full
    /// DOFs, e.g. a driven operator's interior DOFs).
    pub(crate) fn restrict_rows(&self, rows: &[usize]) -> Self {
        let rows_v: Vec<Vec<(usize, c64)>> = rows.iter().map(|&i| self.row(i).collect()).collect();
        let shift = rows.iter().map(|&i| self.shift[i]).collect();
        let mut out = Self::from_rows(self.n_reduced, rows_v, shift);
        out.base = rows
            .iter()
            .flat_map(|&i| {
                self.base[self.row_ptr[i]..self.row_ptr[i + 1]]
                    .iter()
                    .copied()
            })
            .collect();
        out.bloch_k = self.bloch_k;
        out
    }
}

/// Counts of a [`PeriodicConstraint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeriodicConstraintReport {
    /// Full H(curl) DOFs.
    pub n_full: usize,
    /// Reduced H(curl) DOFs.
    pub n_reduced: usize,
    /// Full DOFs aliased onto another DOF (slave copies).
    pub n_aliased: usize,
    /// Full DOFs eliminated as PEC (including slave copies of PEC masters).
    pub n_eliminated: usize,
    /// Periodic DOF classes where PEC covered some copies but not all
    /// (all copies eliminated; a **warning**).
    pub n_one_sided_pec: usize,
    /// Full nodes.
    pub n_nodes: usize,
    /// Reduced (kept, non-PEC) nodes.
    pub n_nodes_reduced: usize,
}

/// The periodic DOF constraint on an H(curl) space (see the
/// [module docs](self)).
#[derive(Debug, Clone)]
pub struct PeriodicConstraint {
    order: ElementOrder,
    dofs: DofAliasMap,
    nodes: DofAliasMap,
    /// Global edges (for the gradient).
    edges: Vec<[u32; 2]>,
    /// Full DOFs on a periodic face (masters and slaves).
    on_periodic: Vec<bool>,
    /// Full DOFs that are their class's canonical master (or not periodic).
    is_master: Vec<bool>,
    report: PeriodicConstraintReport,
}

impl PeriodicConstraint {
    /// Build the constraint for `space` from `map`, with an optional PEC
    /// interior mask over `space.n_dofs()` (`true` = kept, as
    /// [`HcurlSpace::pec_interior_mask`]).
    ///
    /// # Errors
    ///
    /// - [`PeriodicError::Unsupported`] for a p=2 space (the face-block
    ///   pairing is owned by the p=2 epic #836);
    /// - [`PeriodicError::Mismatch`] if `map`, `space` or the mask describe
    ///   different meshes.
    pub fn build(
        space: &HcurlSpace,
        map: &PeriodicMap,
        pec_interior_mask: Option<&[bool]>,
    ) -> Result<Self, PeriodicError> {
        if space.order() != ElementOrder::P1 {
            return Err(PeriodicError::Unsupported {
                feature: "a p=2 (or higher) H(curl) space",
                phase: "the p=2 face-block pairing lands with Epic #836; use ElementOrder::P1",
            });
        }
        if space.n_nodes() != map.n_nodes() || space.n_edges() != map.n_edges() {
            return Err(PeriodicError::Mismatch(format!(
                "the space has {} nodes / {} edges, the periodic map {} / {}",
                space.n_nodes(),
                space.n_edges(),
                map.n_nodes(),
                map.n_edges()
            )));
        }
        let n_full = space.n_dofs();
        if let Some(m) = pec_interior_mask
            && m.len() != n_full
        {
            return Err(PeriodicError::Mismatch(format!(
                "PEC mask has {} entries, the space has {n_full} DOFs",
                m.len()
            )));
        }
        let kept = |i: usize| pec_interior_mask.is_none_or(|m| m[i]);

        // --- Edge (p=1 DOF) classes -----------------------------------------
        // Class key = canonical master edge. A class is PEC if any member is.
        let master_of = |e: usize| map.edge_alias(e).map_or(e, |a| a.master);
        let mut class_pec = vec![false; n_full];
        let mut class_any_kept = vec![false; n_full];
        let on_periodic: Vec<bool> = (0..n_full).map(|e| map.edge_alias(e).is_some()).collect();
        for e in 0..n_full {
            let m = master_of(e);
            if kept(e) {
                class_any_kept[m] = true;
            } else {
                class_pec[m] = true;
            }
        }
        let mut n_one_sided_pec = 0usize;
        for e in 0..n_full {
            if master_of(e) == e && on_periodic[e] && class_pec[e] && class_any_kept[e] {
                n_one_sided_pec += 1;
            }
        }
        // Reduced numbering: canonical masters (and non-periodic DOFs) in
        // ascending full order.
        let mut red = vec![usize::MAX; n_full];
        let mut n_reduced = 0usize;
        for e in 0..n_full {
            if master_of(e) == e && !class_pec[e] {
                red[e] = n_reduced;
                n_reduced += 1;
            }
        }
        let mut rows = Vec::with_capacity(n_full);
        let mut shift = Vec::with_capacity(n_full);
        let (mut n_aliased, mut n_eliminated) = (0usize, 0usize);
        for e in 0..n_full {
            let (m, sign, d) = map
                .edge_alias(e)
                .map_or((e, 1i8, [0.0; 3]), |a| (a.master, a.sign, a.shift));
            shift.push(d);
            if class_pec[m] {
                n_eliminated += 1;
                rows.push(Vec::new());
            } else {
                if m != e {
                    n_aliased += 1;
                }
                rows.push(vec![(red[m], c64::new(f64::from(sign), 0.0))]);
            }
        }
        let dofs = DofAliasMap::from_rows(n_reduced, rows, shift);
        let is_master: Vec<bool> = (0..n_full).map(|e| master_of(e) == e).collect();

        // --- Node classes (P1, for the gradient) ----------------------------
        let n_nodes = space.n_nodes();
        let mut node_pec = vec![false; n_nodes];
        for (e, ab) in space.edges().iter().enumerate() {
            if !kept(e) {
                node_pec[ab[0] as usize] = true;
                node_pec[ab[1] as usize] = true;
            }
        }
        let node_master = |v: usize| map.node_alias(v).map_or(v, |a| a.master);
        let mut node_class_pec = vec![false; n_nodes];
        for v in 0..n_nodes {
            if node_pec[v] {
                node_class_pec[node_master(v)] = true;
            }
        }
        let mut node_red = vec![usize::MAX; n_nodes];
        let mut n_nodes_reduced = 0usize;
        for v in 0..n_nodes {
            if node_master(v) == v && !node_class_pec[v] {
                node_red[v] = n_nodes_reduced;
                n_nodes_reduced += 1;
            }
        }
        let mut nrows = Vec::with_capacity(n_nodes);
        let mut nshift = Vec::with_capacity(n_nodes);
        for v in 0..n_nodes {
            let (m, d) = map
                .node_alias(v)
                .map_or((v, [0.0; 3]), |a| (a.master, a.shift));
            nshift.push(d);
            if node_class_pec[m] {
                nrows.push(Vec::new());
            } else {
                nrows.push(vec![(node_red[m], c64::new(1.0, 0.0))]);
            }
        }
        let nodes = DofAliasMap::from_rows(n_nodes_reduced, nrows, nshift);

        Ok(Self {
            order: space.order(),
            dofs,
            nodes,
            edges: space.edges().to_vec(),
            on_periodic,
            is_master,
            report: PeriodicConstraintReport {
                n_full,
                n_reduced,
                n_aliased,
                n_eliminated,
                n_one_sided_pec,
                n_nodes,
                n_nodes_reduced,
            },
        })
    }

    /// Element order of the space the constraint was built on (P1 in this
    /// phase).
    pub fn order(&self) -> ElementOrder {
        self.order
    }

    /// The H(curl) DOF prolongation `P_edge`.
    pub fn dofs(&self) -> &DofAliasMap {
        &self.dofs
    }

    /// The P1 node prolongation `P_node` (gauge / gradient nullspace).
    pub fn nodes(&self) -> &DofAliasMap {
        &self.nodes
    }

    /// Full H(curl) DOF count.
    pub fn n_full(&self) -> usize {
        self.dofs.n_full()
    }

    /// Reduced H(curl) DOF count.
    pub fn n_reduced(&self) -> usize {
        self.dofs.n_reduced()
    }

    /// The alias of full DOF `i` (see [`DofAliasMap::alias`]).
    pub fn alias(&self, i: usize) -> DofAlias {
        self.dofs.alias(i)
    }

    /// Whether full DOF `i` lies on a periodic face (master or slave copy).
    pub fn is_periodic_dof(&self, i: usize) -> bool {
        self.on_periodic[i]
    }

    /// `Pᴴ A P` (see [`DofAliasMap::reduce_matrix`]).
    ///
    /// # Errors
    ///
    /// As [`DofAliasMap::reduce_matrix`].
    pub fn reduce_matrix<T: PeriodicScalar>(
        &self,
        a: SparseColMatRef<'_, usize, T>,
    ) -> Result<SparseColMat<usize, T>, PeriodicError> {
        self.dofs.reduce_matrix(a)
    }

    /// `Pᴴ b` (see [`DofAliasMap::reduce_vector`]).
    pub fn reduce_vector<T: PeriodicScalar>(&self, b: &[T]) -> Vec<T> {
        self.dofs.reduce_vector(b)
    }

    /// `P x_r` (see [`DofAliasMap::expand`]).
    pub fn expand<T: PeriodicScalar>(&self, x_r: &[T]) -> Vec<T> {
        self.dofs.expand(x_r)
    }

    /// `∂P/∂k` coefficients of the H(curl) prolongation (see
    /// [`DofAliasMap::dphase_dk`]).
    pub fn dphase_dk(&self) -> Vec<[c64; 3]> {
        self.dofs.dphase_dk()
    }

    /// The same constraint with the **Bloch phase** of wave vector `k`
    /// (rad per mesh length unit) on both the H(curl) DOFs and the P1
    /// nodes: every alias row becomes `σ · e^{−j k·Σd}` (Epic #837 Phase 2,
    /// issue #858). Edges and nodes carry the **same** phase, so the
    /// reduced gradient keeps `G P_node = P_edge G_r` and the curl-curl
    /// kernel is exactly the Bloch-phased gradients
    /// ([`Self::reduced_gradient_complex`]).
    ///
    /// `k = [0; 3]` reproduces the zero-phase constraint bit for bit. A
    /// complex phase makes `Pᴴ A P` Hermitian, not complex-symmetric:
    /// only Hermitian-safe solvers may consume it
    /// ([`crate::eigen::bloch`]; the direct-LU driven path
    /// [`crate::driven::periodic::PeriodicDrivenOperator`] and the Floquet
    /// unit cell [`crate::driven::floquet`], issue #870).
    pub fn with_bloch_phase(&self, k: [f64; 3]) -> Self {
        let mut out = self.clone();
        out.dofs = self.dofs.with_bloch_phase(k);
        out.nodes = self.nodes.with_bloch_phase(k);
        out
    }

    /// Inverse tripwire of issue #858 (golden 4): the Bloch phase on the
    /// H(curl) DOFs **only**, leaving the node constraint at zero phase.
    /// The reduced gradient then no longer satisfies
    /// `G P_node = P_edge G_r`, so `image(G_r)` is not the curl-curl
    /// kernel. Never use it for a solve.
    #[doc(hidden)]
    pub fn tripwire_bloch_phase_edges_only(&self, k: [f64; 3]) -> Self {
        let mut out = self.clone();
        out.dofs = self.dofs.with_bloch_phase(k);
        out
    }

    /// The Bloch wave vector of the constraint (`[0; 3]` at zero phase).
    pub fn bloch_k(&self) -> [f64; 3] {
        self.dofs.bloch_k()
    }

    /// Whether any coefficient is complex (a Bloch phase that is not a
    /// sign). Real coefficients (`k = 0`, or `k·Σd ∈ πℤ` on every alias)
    /// keep every operator real / complex-symmetric.
    pub fn has_complex_phase(&self) -> bool {
        !(self.dofs.is_real() && self.nodes.is_real())
    }

    /// The counts.
    pub fn report(&self) -> &PeriodicConstraintReport {
        &self.report
    }

    /// Human-readable warnings (one-sided PEC on a periodic face).
    pub fn warnings(&self) -> Vec<String> {
        let mut w = Vec::new();
        if self.report.n_one_sided_pec > 0 {
            w.push(format!(
                "{} periodic edge class(es) are PEC on one periodic face but not on its \
                 partner; both copies were eliminated (periodicity forces the field to vanish \
                 there). Check that the PEC geometry is itself periodic",
                self.report.n_one_sided_pec
            ));
        }
        w
    }

    /// The full discrete gradient `G` (`n_edges × n_nodes`, `G[e, b] = +1`,
    /// `G[e, a] = −1` for edge `e = (a, b)`, `a < b`).
    pub fn full_gradient(&self) -> SparseColMat<usize, f64> {
        let mut tr = Vec::with_capacity(2 * self.edges.len());
        for (e, ab) in self.edges.iter().enumerate() {
            tr.push(Triplet::new(e, ab[0] as usize, -1.0));
            tr.push(Triplet::new(e, ab[1] as usize, 1.0));
        }
        SparseColMat::try_new_from_triplets(self.edges.len(), self.nodes.n_full(), &tr)
            .expect("gradient triplets are in range")
    }

    /// The reduced discrete gradient `G_r` (`n_reduced × n_nodes_reduced`):
    /// row `r` is the gradient row of the canonical master edge of reduced
    /// DOF `r`, with each endpoint replaced by its reduced node.
    ///
    /// It satisfies `G P_node = P_edge G_r` (checked by
    /// [`Self::check_gradient_commutes`]), so `image(G_r)` is exactly the
    /// periodic gradients and `K_r G_r = 0`.
    ///
    /// # Panics
    ///
    /// Panics with a complex (Bloch) phase: the real gradient would drop
    /// the node phases. Use [`Self::reduced_gradient_complex`].
    pub fn reduced_gradient(&self) -> SparseColMat<usize, f64> {
        assert!(
            !self.has_complex_phase(),
            "PeriodicConstraint::reduced_gradient is real; with a Bloch phase use \
             reduced_gradient_complex"
        );
        let mut tr = Vec::new();
        for (e, ab) in self.edges.iter().enumerate() {
            // A canonical master carries coefficient +1 in its own column.
            if !self.is_master[e] {
                continue;
            }
            let DofAlias::Reduced { index, .. } = self.dofs.alias(e) else {
                continue;
            };
            for (v, s) in [(ab[0] as usize, -1.0), (ab[1] as usize, 1.0)] {
                if let DofAlias::Reduced {
                    index: rv,
                    coeff: cv,
                } = self.nodes.alias(v)
                {
                    tr.push(Triplet::new(index, rv, s * cv.re));
                }
            }
        }
        SparseColMat::try_new_from_triplets(self.n_reduced(), self.nodes.n_reduced(), &tr)
            .expect("reduced gradient triplets are in range")
    }

    /// The reduced discrete gradient with the **Bloch-phased** node and
    /// edge constraints (issue #858): row `r` is the gradient row of the
    /// canonical master edge of reduced DOF `r` (coefficient `+1`, zero
    /// lattice shift), each endpoint replaced by its reduced node scaled by
    /// that node's coefficient `e^{−j k·Σd}`. Equal to
    /// [`Self::reduced_gradient`] at zero phase.
    ///
    /// It satisfies `G P_node = P_edge G_r` at any `k` (checked by
    /// [`Self::check_gradient_commutes`]), so `image(G_r)` is exactly the
    /// Bloch-periodic gradients and `K_r G_r = 0`.
    pub fn reduced_gradient_complex(&self) -> SparseColMat<usize, c64> {
        let mut tr = Vec::new();
        for (e, ab) in self.edges.iter().enumerate() {
            if !self.is_master[e] {
                continue;
            }
            let DofAlias::Reduced { index, .. } = self.dofs.alias(e) else {
                continue;
            };
            for (v, s) in [(ab[0] as usize, -1.0), (ab[1] as usize, 1.0)] {
                if let DofAlias::Reduced {
                    index: rv,
                    coeff: cv,
                } = self.nodes.alias(v)
                {
                    tr.push(Triplet::new(index, rv, cv * s));
                }
            }
        }
        SparseColMat::try_new_from_triplets(self.n_reduced(), self.nodes.n_reduced(), &tr)
            .expect("reduced gradient triplets are in range")
    }

    /// Verify `G P_node = P_edge G_r` entry by entry, with
    /// [`Self::reduced_gradient_complex`] (exact integers at zero phase,
    /// round-off with a Bloch phase). Returns the largest absolute entry of
    /// the difference.
    pub fn check_gradient_commutes(&self) -> f64 {
        let g = self.full_gradient();
        let gr = self.reduced_gradient_complex();
        let n_e = self.edges.len();
        let n_vr = self.nodes.n_reduced();
        // Dense-row comparison, one edge at a time (rows are tiny).
        let mut gr_rows: Vec<Vec<(usize, c64)>> = vec![Vec::new(); gr.nrows()];
        for j in 0..gr.ncols() {
            for (i, &v) in gr.row_idx_of_col(j).zip(gr.val_of_col(j)) {
                gr_rows[i].push((j, v));
            }
        }
        let g_rows = csc_rows(g.as_ref());
        let zero = c64::new(0.0, 0.0);
        let mut worst = 0.0_f64;
        for (e, g_row) in g_rows.iter().enumerate().take(n_e) {
            let mut lhs = vec![zero; n_vr];
            for &(v, gv) in g_row {
                for (rv, cv) in self.nodes.row(v) {
                    lhs[rv] += cv * gv;
                }
            }
            let mut rhs = vec![zero; n_vr];
            for (r, c) in self.dofs.row(e) {
                for &(rv, val) in &gr_rows[r] {
                    rhs[rv] += c * val;
                }
            }
            for k in 0..n_vr {
                worst = worst.max((lhs[k] - rhs[k]).norm());
            }
        }
        worst
    }
}

/// Row lists of a CSC matrix.
fn csc_rows(a: SparseColMatRef<'_, usize, f64>) -> Vec<Vec<(usize, f64)>> {
    let mut rows = vec![Vec::new(); a.nrows()];
    for j in 0..a.ncols() {
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            rows[i].push((j, v));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// A two-row map: row 0 a p=1 master, row 1 a p=2 style **block row**
    /// (two entries) shifted by `d = (1, 0, 0)`.
    fn block_map() -> DofAliasMap {
        let one = c64::new(1.0, 0.0);
        DofAliasMap::from_rows(
            2,
            vec![vec![(0, one)], vec![(0, one), (1, c64::new(-0.5, 0.0))]],
            vec![[0.0; 3], [1.0, 0.0, 0.0]],
        )
    }

    /// Issue #915 item 3: comparing prolongations row by row never panics
    /// on a block row (`alias` does), and sees the Bloch phase.
    #[test]
    fn same_prolongation_handles_block_rows() {
        let base = block_map();
        assert!(
            std::panic::catch_unwind(|| base.alias(1)).is_err(),
            "alias panics on a block row"
        );
        assert!(base.same_prolongation(&base.with_bloch_phase([0.0; 3])));
        assert!(base.same_prolongation(&base.with_bloch_phase([2.0 * PI, 5.0, 0.0])));
        assert!(!base.same_prolongation(&base.with_bloch_phase([1e-3, 0.0, 0.0])));
        assert!(!base.same_prolongation(&base.with_bloch_phase([PI, 0.0, 0.0])));
    }

    /// Issue #915 item 2: a reciprocal lattice vector at large `|G|` gives
    /// a phase factor of exactly `1`, consistent with [`wrap_phase`].
    #[test]
    fn lattice_phase_is_exact_at_large_g() {
        for m in [1.0, 16.0, 64.0, 1024.0] {
            let th = 2.0 * PI * m;
            assert_eq!(wrap_phase(th), 0.0, "2π·{m}");
            assert!(
                is_lattice_phase(th, th) && is_lattice_phase(-th, th),
                "2π·{m}"
            );
            let p = block_map().with_bloch_phase([th, 0.0, 0.0]);
            assert!(p.is_real(), "2π·{m}: P(G) is real");
            assert!(p.same_prolongation(&block_map()), "2π·{m}: P(G) = P(0)");
        }
        assert!(!is_lattice_phase(1e-12, 1e-12));
        let th = 2.0 * PI * 64.0 + 1e-9;
        assert!(!is_lattice_phase(th, th));
        assert!(is_lattice_phase(1e-15, 1e-15));
        // Cancelling terms: θ is round-off of the terms, not of itself.
        assert!(is_lattice_phase(5.7e-14, 2.0 * 2.0 * PI * 64.0));
        assert!(!is_lattice_phase(5.7e-14, 5.7e-14));
        // P on a diagonal shift whose two phases cancel.
        let one = c64::new(1.0, 0.0);
        let diag = DofAliasMap::from_rows(
            1,
            vec![vec![(0, one)], vec![(0, one)]],
            vec![[0.0; 3], [0.7, 1.3, 0.0]],
        );
        let k = [2.0 * PI * 64.0 / 0.7, -2.0 * PI * 64.0 / 1.3, 0.0];
        assert!(diag.with_bloch_phase(k).same_prolongation(&diag));
    }
}
