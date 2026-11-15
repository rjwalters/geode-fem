//! Smoothed-aggregation **algebraic multigrid** (SA-AMG) for the SPD nodal
//! coarse operators of the eigen and electrostatic paths (issue #565).
//!
//! The hierarchy is built once from an SPD operator in CSC form and applied as
//! a fixed number of **symmetric** V-cycles per solve. That makes each apply a
//! fixed linear SPD operator — a valid CG / MINRES preconditioner — costing
//! `O(nnz(A))` work, with no global factor beyond the tiny coarsest level:
//!
//! - [`AmgConfig`] — the tuning knobs, including the `GEODE_AMG_*` env seam.
//! - [`CsrOp`] — one level operator in CSR with its cached (inverse-)diagonal,
//!   supplying the three multigrid primitives (matvec, residual, symmetric
//!   Gauss-Seidel smoothing). Also reused as the optional SGS edge smoother of
//!   [`crate::eigen::ams::AmsLitePreconditioner`].
//! - [`AmgCoarseSolver`] — the hierarchy itself: greedy aggregation, a
//!   damped-Jacobi-smoothed piecewise-constant prolongator, the Galerkin triple
//!   product `P^T A P`, and a direct LU at the coarsest level. Consumed by
//!   [`crate::eigen::ams`] as the `CoarseSolve::Amg` coarse solver of the
//!   AMS-lite (Hiptmair-Xu) cycle.
//! - [`ScalarAmg`] — the crate-internal facade for the AMG-preconditioned CG of
//!   the electrostatic capacitance extraction (issue #1051), which wants the
//!   multigrid without any auxiliary-space Maxwell machinery around it.
//!
//! Split out of [`crate::eigen::ams`] by issue #1595 as pure code motion, so
//! that this general-purpose scalar AMG no longer lives inside the
//! Maxwell-specific AMS module and the [`ScalarAmg`] facade's separation from
//! the AMS coarse-solve internals is structural rather than documentary.

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::ams::{galerkin_triplets, lu_solve};
use crate::eigen::dense::EigenError;
use crate::sparse_ops::{spmv_into, spmv_transpose_into};

/// Tuning for the smoothed-aggregation AMG coarse solver (issue #565).
///
/// Every knob has a conservative default suited to the SPD nodal-Laplacian-like
/// coarse operators (`C = Gᵀ(K+|σ|M)G ≈ |σ|·GᵀMG`, a weighted graph Laplacian —
/// the friendly case for SA-AMG). The `GEODE_AMG_*` env knobs let the σ=4.5
/// characterization run tune the cycle without a recompile; unset ⇒ the defaults.
#[derive(Clone, Copy, Debug)]
pub(super) struct AmgConfig {
    /// Strength-of-connection threshold `θ` (Vaněk smoothed aggregation): node
    /// `j` is strongly coupled to `i` when `A_ij² ≥ θ²·A_ii·A_jj`.
    pub(super) theta: f64,
    /// Pre-smoothing symmetric-GS sweeps per level (each = forward + backward).
    pub(super) pre_sweeps: usize,
    /// Post-smoothing symmetric-GS sweeps per level.
    pub(super) post_sweeps: usize,
    /// Number of V-cycles per coarse solve (a fixed linear, SPD operator).
    pub(super) cycles: usize,
    /// Coarsest level size: at or below this the level is solved by a direct LU.
    pub(super) max_coarse: usize,
    /// Hard cap on the number of coarsening levels (a non-progress guard).
    pub(super) max_levels: usize,
    /// Smooth the tentative (piecewise-constant) prolongator by one damped-Jacobi
    /// pass `P = (I − ω D⁻¹A) P₀`. Smoothed aggregation converges far faster than
    /// plain aggregation; disable only for debugging.
    pub(super) smooth_prolongator: bool,
    /// Density cap (avg nonzeros per row) above which prolongator smoothing is
    /// skipped for that level (falling back to plain, still-multilevel
    /// aggregation). The smoothed Galerkin triple product `PᵀAP` costs
    /// `O(nnz(A)·(rows-per-P)²)`; on a **dense** operator (the vector-nodal
    /// `Πᵀ A Π` has ~150 nnz/row) smoothing would blow the coarse assembly up,
    /// while on the **sparse** gradient operator `C = Gᵀ A G` (~7 nnz/row)
    /// smoothing is cheap and worth it. Gating by density gives smoothed
    /// aggregation on `C` and plain aggregation on `Π` automatically.
    pub(super) max_smooth_density: usize,
}

impl Default for AmgConfig {
    fn default() -> Self {
        Self {
            theta: 0.08,
            pre_sweeps: 1,
            post_sweeps: 1,
            cycles: 2,
            max_coarse: 40,
            max_levels: 25,
            smooth_prolongator: true,
            max_smooth_density: 40,
        }
    }
}

impl AmgConfig {
    /// Defaults overlaid with the optional `GEODE_AMG_{CYCLES,SWEEPS,THETA}`
    /// environment knobs (all inert unless set — the σ=4.5 run's tuning seam).
    pub(super) fn from_env() -> Self {
        let mut c = Self::default();
        if let Some(x) = std::env::var("GEODE_AMG_CYCLES")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            c.cycles = x;
        }
        if let Some(x) = std::env::var("GEODE_AMG_SWEEPS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            c.pre_sweeps = x;
            c.post_sweeps = x;
        }
        if let Some(x) = std::env::var("GEODE_AMG_THETA")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            c.theta = x;
        }
        c
    }
}

/// A single level's operator held in CSR (row-compressed) form with its
/// inverse-diagonal, supporting the three multigrid primitives: sparse matvec,
/// residual, and in-place **symmetric** Gauss–Seidel smoothing of an existing
/// iterate (forward sweep then backward sweep per requested sweep).
pub(super) struct CsrOp {
    n: usize,
    row_ptr: Vec<usize>,
    col_idx: Vec<usize>,
    val: Vec<f64>,
    /// Inverse main diagonal `1 / A_ii` (fallback `1.0` on a zero pivot).
    inv_diag: Vec<f64>,
    /// Main diagonal `A_ii` (kept for the strength-of-connection test).
    diag: Vec<f64>,
}

impl CsrOp {
    /// Transpose a CSC operator into CSR (the operator is symmetric here, so this
    /// is just a layout copy) and cache its (inverse-)diagonal.
    pub(super) fn from_csc(a: SparseColMatRef<'_, usize, f64>) -> Self {
        let n = a.ncols();
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                rows[row_idx[k]].push((j, val[k]));
            }
        }
        let nnz: usize = rows.iter().map(|r| r.len()).sum();
        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut col_idx = Vec::with_capacity(nnz);
        let mut vals = Vec::with_capacity(nnz);
        let mut inv_diag = vec![1.0_f64; n];
        let mut diag = vec![0.0_f64; n];
        row_ptr.push(0);
        for (i, row) in rows.iter().enumerate() {
            let mut dii = 0.0;
            for &(j, v) in row {
                col_idx.push(j);
                vals.push(v);
                if j == i {
                    dii += v;
                }
            }
            diag[i] = dii;
            if dii.abs() > 0.0 {
                inv_diag[i] = 1.0 / dii;
            }
            row_ptr.push(col_idx.len());
        }
        Self {
            n,
            row_ptr,
            col_idx,
            val: vals,
            inv_diag,
            diag,
        }
    }

    /// `y = A · x` (overwrite).
    fn spmv(&self, x: &[f64], y: &mut [f64]) {
        for (i, yi) in y.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s += self.val[k] * x[self.col_idx[k]];
            }
            *yi = s;
        }
    }

    /// `r = b − A · x` (overwrite).
    fn residual(&self, b: &[f64], x: &[f64], r: &mut [f64]) {
        for i in 0..self.n {
            let mut s = b[i];
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s -= self.val[k] * x[self.col_idx[k]];
            }
            r[i] = s;
        }
    }

    /// One Gauss–Seidel update of row `i` on the current iterate `x`:
    /// `x_i ← D_ii⁻¹ (b_i − Σ_{j≠i} A_ij x_j)`.
    #[inline]
    fn gs_row(&self, i: usize, b: &[f64], x: &mut [f64]) {
        let mut s = b[i];
        for k in self.row_ptr[i]..self.row_ptr[i + 1] {
            let j = self.col_idx[k];
            if j != i {
                s -= self.val[k] * x[j];
            }
        }
        x[i] = self.inv_diag[i] * s;
    }

    /// `sweeps` symmetric Gauss–Seidel sweeps on the existing iterate `x` (each
    /// sweep = one forward pass then one backward pass). Symmetric GS on an SPD
    /// operator has a symmetric, convergent error-propagation operator, which is
    /// what keeps the enclosing V-cycle SPD when the pre- and post-smoother match.
    pub(super) fn smooth(&self, b: &[f64], x: &mut [f64], sweeps: usize) {
        for _ in 0..sweeps {
            for i in 0..self.n {
                self.gs_row(i, b, x);
            }
            for i in (0..self.n).rev() {
                self.gs_row(i, b, x);
            }
        }
    }
}

/// One level of the AMG hierarchy: the level operator plus the prolongation `P`
/// (`n_fine × n_coarse`) to the next-coarser level. `P·x` (prolong) and `Pᵀ·r`
/// (restrict) are the free CSC [`spmv`] / [`spmv_transpose_into`].
struct AmgLevel {
    a: CsrOp,
    p: SparseColMat<usize, f64>,
}

/// A smoothed-aggregation **algebraic multigrid** coarse solver (issue #565).
///
/// Built once from an SPD coarse operator (the nodal `C = Gᵀ(K+|σ|M)G` or the
/// vector-nodal `Πᵀ(K+|σ|M)Π`). Coarsening recurses — greedy aggregation forms
/// coarse DOFs, a damped-Jacobi-smoothed piecewise-constant prolongator `P`
/// spreads their support, and the Galerkin triple product `PᵀAP` builds the next
/// level's operator — until the level is small enough for a direct LU. Each
/// [`Self::solve`] runs a fixed number of **symmetric** V-cycles (pre-smooth,
/// restrict, recurse, prolong, post-smooth) from a zero start, which is a fixed
/// linear SPD operator (a valid MINRES/CG preconditioner) and `O(nnz(A))` — i.e.
/// `O(node_dim)` — work per apply, with **no** global edge-space or even a global
/// node-space factor beyond the tiny coarsest level.
///
/// Unlike the fixed-sweep [`crate::eigen::ams::SgsCoarseSolver`] (which a
/// bounded sweep count leaves a low-frequency coarse-error tail — the ~1e-5
/// σ=4.5 plateau of #562), the recursion to a direct coarsest solve removes
/// every error frequency, so the coarse correction is a genuine approximate
/// inverse rather than a few relaxation steps.
pub(crate) struct AmgCoarseSolver {
    /// Finest → second-coarsest levels (each carries its prolongation `P`).
    levels: Vec<AmgLevel>,
    /// Direct LU of the coarsest-level operator.
    coarsest: Lu<usize, f64>,
    /// Finest operator dimension.
    n: usize,
    /// V-cycles per [`Self::solve`].
    cycles: usize,
    pre_sweeps: usize,
    post_sweeps: usize,
}

impl AmgCoarseSolver {
    /// Build the AMG hierarchy from an SPD coarse operator in CSC.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if a Galerkin coarse assembly or the
    /// coarsest-level sparse LU fails.
    pub(super) fn from_csc(
        a: SparseColMatRef<'_, usize, f64>,
        cfg: AmgConfig,
    ) -> Result<Self, EigenError> {
        let n = a.ncols();
        let mut levels: Vec<AmgLevel> = Vec::new();
        let mut current: SparseColMat<usize, f64> = csc_owned(a);
        loop {
            let dim = current.ncols();
            if levels.len() >= cfg.max_levels || dim <= cfg.max_coarse {
                break;
            }
            let csr = CsrOp::from_csc(current.as_ref());
            let (agg, n_c) = aggregate(&csr, cfg.theta);
            // No coarsening progress (or degenerate) ⇒ stop and direct-solve here.
            if n_c >= dim || n_c == 0 {
                break;
            }
            let p = build_prolongator(&csr, &agg, n_c, &cfg)?;
            let a_c = galerkin(p.as_ref(), current.as_ref(), n_c)?;
            levels.push(AmgLevel { a: csr, p });
            current = a_c;
        }
        let coarsest = current
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("AMG coarsest sparse LU: {e:?}")))?;
        Ok(Self {
            levels,
            coarsest,
            n,
            cycles: cfg.cycles.max(1),
            pre_sweeps: cfg.pre_sweeps,
            post_sweeps: cfg.post_sweeps,
        })
    }

    /// Number of coarsening levels above the direct coarsest solve — `≥ 1` iff
    /// the operator actually coarsened (used by the "genuinely multilevel" test).
    #[cfg(test)]
    pub(super) fn num_levels(&self) -> usize {
        self.levels.len()
    }

    /// Approximately solve `A · out = b` with `cycles` symmetric V-cycles from a
    /// zero start (`out` overwritten, length `n`).
    pub(super) fn solve(&self, b: &[f64], out: &mut [f64]) {
        out.iter_mut().for_each(|v| *v = 0.0);
        if self.levels.is_empty() {
            // Operator was already at/under the coarsest threshold ⇒ exact solve.
            lu_solve(&self.coarsest, b, out);
            return;
        }
        let mut r = b.to_vec();
        let mut correction = vec![0.0_f64; self.n];
        for cyc in 0..self.cycles {
            correction.iter_mut().for_each(|v| *v = 0.0);
            self.vcycle(0, &r, &mut correction);
            for (o, &c) in out.iter_mut().zip(correction.iter()) {
                *o += c;
            }
            if cyc + 1 < self.cycles {
                self.levels[0].a.residual(b, out, &mut r);
            }
        }
    }

    /// One symmetric V-cycle on level `lvl`. `x` must be zeroed on entry (the
    /// caller supplies a fresh coarse correction), so the pre-smooth starts from a
    /// zero guess. Pre- and post-smoother match (symmetric GS), and the coarsest
    /// level is a direct solve, so the cycle is symmetric and — for the SPD
    /// operator — positive definite.
    fn vcycle(&self, lvl: usize, b: &[f64], x: &mut [f64]) {
        if lvl == self.levels.len() {
            lu_solve(&self.coarsest, b, x);
            return;
        }
        let lev = &self.levels[lvl];
        // Pre-smooth.
        lev.a.smooth(b, x, self.pre_sweeps);
        // Restrict the residual to the coarse level.
        let mut r = vec![0.0_f64; lev.a.n];
        lev.a.residual(b, x, &mut r);
        let nc = lev.p.ncols();
        let mut rc = vec![0.0_f64; nc];
        spmv_transpose_into(lev.p.as_ref(), &r, &mut rc);
        // Recurse (coarse correction from a zero coarse guess).
        let mut xc = vec![0.0_f64; nc];
        self.vcycle(lvl + 1, &rc, &mut xc);
        // Prolong and add.
        let mut pxc = vec![0.0_f64; lev.a.n];
        spmv_into(lev.p.as_ref(), &xc, &mut pxc);
        for (xi, &pi) in x.iter_mut().zip(pxc.iter()) {
            *xi += pi;
        }
        // Post-smooth.
        lev.a.smooth(b, x, self.post_sweeps);
    }
}

/// The smoothed-aggregation AMG of [`AmgCoarseSolver`] as a stand-alone
/// approximate inverse for a scalar SPD operator, with the default tuning
/// (`GEODE_AMG_*` env knobs ignored) and a chosen V-cycle count per apply.
///
/// Crate-internal facade for the AMG-preconditioned CG of the electrostatic
/// capacitance extraction (issue #1051), so that path does not reach into
/// the coarse-solve internals of the AMS cycle.
pub(crate) struct ScalarAmg(AmgCoarseSolver);

impl ScalarAmg {
    /// Build the hierarchy for the SPD operator `a` (CSC).
    ///
    /// # Errors
    ///
    /// As [`AmgCoarseSolver::from_csc`].
    pub(crate) fn new(
        a: SparseColMatRef<'_, usize, f64>,
        cycles: usize,
    ) -> Result<Self, EigenError> {
        AmgCoarseSolver::from_csc(
            a,
            AmgConfig {
                cycles: cycles.max(1),
                ..AmgConfig::default()
            },
        )
        .map(Self)
    }

    /// `out ≈ A⁻¹ b`: a fixed linear SPD operator (valid as a CG
    /// preconditioner).
    pub(crate) fn apply(&self, b: &[f64], out: &mut [f64]) {
        self.0.solve(b, out);
    }

    /// Coarsening levels above the direct coarsest solve.
    pub(crate) fn n_levels(&self) -> usize {
        self.0.levels.len()
    }
}

/// Copy a borrowed CSC operator into an owned [`SparseColMat`] (the AMG builder
/// needs an owned finest level so the coarsening loop is uniform).
fn csc_owned(a: SparseColMatRef<'_, usize, f64>) -> SparseColMat<usize, f64> {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(val.len());
    for j in 0..a.ncols() {
        for k in col_ptr[j]..col_ptr[j + 1] {
            trips.push(Triplet::new(row_idx[k], j, val[k]));
        }
    }
    SparseColMat::try_new_from_triplets(a.nrows(), a.ncols(), &trips)
        .expect("copy of a valid CSC operator cannot fail")
}

/// Greedy Vaněk aggregation on the strength graph of an SPD operator. Returns the
/// aggregate index of every fine DOF and the aggregate count `n_c`.
///
/// Phase 1 seeds aggregates from fully-unaggregated neighborhoods; phase 2 sweeps
/// leftover DOFs into their strongest already-seeded aggregate; phase 3 makes any
/// remaining DOFs singleton aggregates. Every DOF ends in exactly one aggregate.
fn aggregate(a: &CsrOp, theta: f64) -> (Vec<usize>, usize) {
    let n = a.n;
    let t2 = theta * theta;
    // Strong-neighbor lists (with the coupling magnitude for phase-2 ranking).
    let mut strong: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for (i, si) in strong.iter_mut().enumerate() {
        let dii = a.diag[i].abs();
        if dii <= 0.0 {
            continue;
        }
        for k in a.row_ptr[i]..a.row_ptr[i + 1] {
            let j = a.col_idx[k];
            if j == i {
                continue;
            }
            let djj = a.diag[j].abs();
            let aij = a.val[k];
            if djj > 0.0 && aij * aij >= t2 * dii * djj {
                si.push((j, aij.abs()));
            }
        }
    }

    // status: 0 = unassigned, 1 = seed-member, 2 = swept-in.
    let mut status = vec![0u8; n];
    let mut agg = vec![usize::MAX; n];
    let mut n_agg = 0usize;

    // Phase 1: seed aggregates from unaggregated neighborhoods.
    for i in 0..n {
        if status[i] != 0 {
            continue;
        }
        if strong[i].iter().all(|&(j, _)| status[j] == 0) {
            agg[i] = n_agg;
            status[i] = 1;
            for &(j, _) in &strong[i] {
                agg[j] = n_agg;
                status[j] = 1;
            }
            n_agg += 1;
        }
    }

    // Phase 2: attach leftovers to their strongest seeded aggregate.
    for i in 0..n {
        if status[i] != 0 {
            continue;
        }
        let mut best = 0.0_f64;
        let mut best_agg = None;
        for &(j, mag) in &strong[i] {
            if status[j] == 1 && mag > best {
                best = mag;
                best_agg = Some(agg[j]);
            }
        }
        if let Some(g) = best_agg {
            agg[i] = g;
            status[i] = 2;
        }
    }

    // Phase 3: any still-unassigned DOF becomes its own aggregate.
    for i in 0..n {
        if status[i] == 0 {
            agg[i] = n_agg;
            status[i] = 1;
            n_agg += 1;
        }
    }

    (agg, n_agg)
}

/// Build the (optionally Jacobi-smoothed) prolongator `P` (`n × n_c`) from an
/// aggregation. The tentative prolongator `P₀` is the piecewise-constant
/// aggregate indicator, column-normalized (`1/√|aggregate|`) so the constant
/// near-null-space is represented exactly. When `cfg.smooth_prolongator` is set,
/// one damped-Jacobi pass `P = (I − ω D⁻¹A) P₀` (with `ω = 4/(3ρ)`, `ρ` the
/// estimated spectral radius of `D⁻¹A`) spreads the support — smoothed
/// aggregation, which converges much faster than plain aggregation.
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the `P` triplets.
fn build_prolongator(
    a: &CsrOp,
    agg: &[usize],
    n_c: usize,
    cfg: &AmgConfig,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = a.n;
    let mut size = vec![0usize; n_c];
    for &g in agg {
        size[g] += 1;
    }
    let w: Vec<f64> = size
        .iter()
        .map(|&s| if s > 0 { 1.0 / (s as f64).sqrt() } else { 1.0 })
        .collect();

    // Smooth only when requested AND the level is sparse enough that the
    // Galerkin triple product stays cheap (dense levels — the Πᵀ A Π block —
    // fall back to plain aggregation, which keeps `PᵀAP` at `O(nnz(A))`).
    let avg_density = a.col_idx.len() / a.n.max(1);
    let smooth = cfg.smooth_prolongator && avg_density <= cfg.max_smooth_density;

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    if !smooth {
        for i in 0..n {
            trips.push(Triplet::new(i, agg[i], w[agg[i]]));
        }
    } else {
        let rho = estimate_spectral_radius(a);
        let omega = if rho > 0.0 { 4.0 / (3.0 * rho) } else { 0.0 };
        // Row-wise P = P₀ − ω D⁻¹ (A P₀). For row i, accumulate (A P₀)[i, ·] over
        // the coarse columns its stencil touches, then form the smoothed row.
        let mut acc = vec![0.0_f64; n_c];
        let mut marked = vec![false; n_c];
        let mut touched: Vec<usize> = Vec::new();
        for i in 0..n {
            touched.clear();
            for k in a.row_ptr[i]..a.row_ptr[i + 1] {
                let col = a.col_idx[k];
                let g = agg[col];
                if !marked[g] {
                    marked[g] = true;
                    touched.push(g);
                }
                acc[g] += a.val[k] * w[g];
            }
            for &g in &touched {
                let mut val = -omega * a.inv_diag[i] * acc[g];
                if g == agg[i] {
                    val += w[g];
                }
                if val != 0.0 {
                    trips.push(Triplet::new(i, g, val));
                }
                acc[g] = 0.0;
                marked[g] = false;
            }
        }
    }

    SparseColMat::try_new_from_triplets(n, n_c, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("AMG prolongator assembly: {e:?}")))
}

/// Galerkin coarse operator `A_c = Pᵀ A P` (`n_c × n_c`), reusing the same
/// outer-product triplet accumulation as `Gᵀ A G` with `P`'s rows in place of
/// `G`'s. `A` stays SPD under the triple product (P is full column rank).
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the coarse triplets.
fn galerkin(
    p: SparseColMatRef<'_, usize, f64>,
    a: SparseColMatRef<'_, usize, f64>,
    n_c: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let edge_dim = p.nrows();
    let mut p_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
    let cp = p.col_ptr();
    let ri = p.row_idx();
    let vv = p.val();
    for col in 0..p.ncols() {
        for k in cp[col]..cp[col + 1] {
            p_rows[ri[k]].push((col, vv[k]));
        }
    }
    let trips = galerkin_triplets(&p_rows, &[(a, 1.0)], n_c);
    SparseColMat::try_new_from_triplets(n_c, n_c, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("AMG Galerkin PᵀAP: {e:?}")))
}

/// Estimate the spectral radius of `D⁻¹A` by a few power iterations (used to set
/// the prolongator-smoothing weight `ω = 4/(3ρ)`). `D⁻¹A` is similar to the
/// symmetric `D^{-1/2} A D^{-1/2}`, so the power method converges to its largest
/// eigenvalue; a handful of iterations is enough for the weight.
fn estimate_spectral_radius(a: &CsrOp) -> f64 {
    let n = a.n;
    if n == 0 {
        return 1.0;
    }
    let mut x: Vec<f64> = (0..n).map(|i| 1.0 + ((i % 7) as f64) * 0.1).collect();
    let mut ax = vec![0.0_f64; n];
    let mut lambda = 1.0_f64;
    for _ in 0..8 {
        a.spmv(&x, &mut ax);
        for (axi, &idi) in ax.iter_mut().zip(a.inv_diag.iter()) {
            *axi *= idi;
        }
        let nrm = ax.iter().map(|v| v * v).sum::<f64>().sqrt();
        let xnrm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        if nrm <= 0.0 || xnrm <= 0.0 {
            return 1.0;
        }
        lambda = nrm / xnrm;
        let inv = 1.0 / nrm;
        for (xi, &axi) in x.iter_mut().zip(ax.iter()) {
            *xi = axi * inv;
        }
    }
    if lambda > 0.0 { lambda } else { 1.0 }
}
