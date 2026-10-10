//! The **share-free** TM cutoff of a p=2 wave port's guide section (issue
//! #955, option 3), and the TM guard it supports. An **honest negative**:
//! measured, it does not recover single-mode band safely. Opt-in; nothing
//! in the crate or the CLI calls it.
//!
//! # The quantity
//!
//! [`share_free_tm_cutoff`] computes, on a guide section closed with PEC on
//! its whole boundary,
//!
//! ```text
//!   k_TM,h² = min ‖curl u‖² / ‖u·n‖²   over the discretely divergence-free
//!                                       p=2 fields u (Gᵀ M u = 0),
//! ```
//!
//! with `n` the port normal. In the continuum it is the guide's TM cutoff
//! `k_c²` at any section depth: the `β = 0` TM field attains it, and on a
//! convex section `‖curl u‖² ≥ ‖∇u‖² ≥ ‖∇_t u_n‖² ≥ k_c²‖u_n‖²` for every
//! divergence-free `u` with no tangential trace. On a structured `2 × 1`
//! guide it is TM₁₁ = 3.5124 to 0.03 %.
//!
//! **Closure at the cut: PEC**, as in option 1. The `β = 0` TM field is
//! normal to a cap, so a PEC cap admits it. A natural (free) cap does not
//! work with this constraint: orthogonality to the gradients of scalars
//! that are free on the cap imposes `u·n = 0` there, which removes that
//! field and raises the minimum.
//!
//! # What it proves, and why that is weak near the cutoff
//!
//! Every discrete resonance `(k, u)` of the section with `k > 0` is
//! `M`-orthogonal to the gradients, so it is admissible, and its quotient is
//! `k²/s` with `s = ‖u·n‖²/‖u‖²` its exact axial share. Hence
//!
//! ```text
//!   s ≤ (k / k_TM,h)²   for every resonance of the section.
//! ```
//!
//! The cutoff bounds the **share** of a mode, not its frequency. A mode at
//! `0.95·k_TM,h` may carry up to 90 % of its energy in `E·n`. A mode with
//! share at least `s₀` lies at or above `√s₀·k_TM,h`, and no higher bound
//! on its frequency follows. The tests check the bound on every row of the
//! measurement table where the section is the whole box (worst ratio 1.000,
//! reached by a pure TM mode) and on every resolved mode of the `3 × 1`
//! fixture.
//!
//! # Measured (`tests/wave_port_p2.rs`, Gmsh 4.15.2)
//!
//! The reference is the box's lowest p=2 mode whose **sampled** `E_z`
//! share is at least 0.4, the classifier of the #905 tables. Both tables
//! are in `benchmarks/tm_guard_955/`.
//!
//! - **The cutoff used directly is unsafe.** As `0.98·min(k_TM,h, k_face)`
//!   (the margin of option 1) it is at or above the reference on 213 of the
//!   815 rows of `tm_guard_p2_measurement_table` (worst 8.45 % above,
//!   `gmsh 3×1×4.13`, `lc` 0.9), and on 164 of the 216 long guides of
//!   `tm_guard_p2_long_guide_table` (23 modes within reach, 110 straddling
//!   the cut, 31 beyond it). The reference modes it misses on the 815-row
//!   table carry 0.43 to 0.95 of their energy in `E_z` (exact share), and
//!   the bound above lets a mode of share 0.43 sit up to 34 % below the
//!   cutoff. The lowest reference there is at `0.895·k_TM,h`.
//! - **The floor `√0.4·k_TM,h` is safe on both tables but loses band.** It
//!   is below the reference on every row of both, including the rows where
//!   the interim law fails (`3 × 1`, `lc` 0.9) and the modes beyond reach
//!   that option 1 misses. But it is below the interim guard on all 130
//!   rows with `k_c·h_n` in `[1.41, 2.5]` (median margin below the face
//!   36.7 % against 21.2 %). On the guides of the issue's band table it
//!   gives up 58.0 % of the single-mode band on the `2 × 1` guide with
//!   `h_n = b` (interim 45.4 %), 74.2 % on the `1.5 × 1` fixture at `lc`
//!   0.92 (interim 64.1 %), and 6.8 % on the `3 × 1` fixture (interim none).
//!
//! The floor is a theorem only for the exact share and for the modes of the
//! section. The reference classifier samples the share at five points per
//! tet without volume weights, and reads some modes far above their exact
//! share (0.50 against 0.11 for the `3×1×8` long-guide mode). On a long
//! guide the box has modes the section does not. On both counts the floor's
//! safety on these tables is measured, not proved. Only the over-reading
//! direction of the sampling is measured. The opposite direction is not: a
//! mode below the reference with exact share at least 0.4 but sampled share
//! under 0.4 would lower the true reference and could hide a miss, of this
//! floor and of the #905 / #990 / #1005 tables. A probe of 12 of the
//! highest-discrepancy rows (sampled minus exact share −0.15 to +0.24 on
//! their reference modes) found no such mode; the tables were not rerun
//! with the exact share as the classifier.
//!
//! # The guard
//!
//! [`PortFaceProjection::guide_tm_guard`] solves the section at
//! [`PortFaceProjection::guide_axial_spacing`]'s window (`reach`) and at
//! [`TM_GUIDE_SHALLOW_FRACTION`] of it, takes `k_c = min(k_deep, k_shallow)`
//! and returns the floor `√`[`TM_LIKE_AXIAL_SHARE`]` · k_c` as
//! [`GuideTmGuard::guard_k_c`], with the cutoff itself in
//! [`TmCutoffSource::ShareFree3d`]. The two depths agree to a median 0.20 %
//! on the 815-row table (at most 10.7 %).
//!
//! Wherever the computation does not apply, the guard is the interim
//! [`TmCutoffEstimate::guard_k_c`] and [`TmCutoffSource::MarginLaw`] says
//! why ([`TmMarginLawReason`]): a p=1 estimate (p=1 is untouched, bit for
//! bit), an open rim, no finite face cutoff, an empty section, or a failed
//! solve.
//!
//! It **enforces nothing**: it returns a value, like the interim guard.
//! Under the operator's rule (never block a reasonable mesh) a caller that
//! warns on it should warn, not reject. It costs a sparse LU of the
//! section's saddle-point matrix and a few dozen Lanczos steps, twice per
//! port: once per port per run, outside the frequency loop. The section is
//! solved empty (geometric `k`, like the face estimate); scale by
//! `1/√(Re ε_n·μ_t)` for a filled guide.

use burn::tensor::backend::Backend;
use faer::Mat;
use faer::c64;
use faer::linalg::solvers::Solve;
use faer::sparse::{SparseColMat, Triplet};

use super::wave_face::{PortFaceProjection, TmCutoffEstimate};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::solve::DrivenMaterials;
use crate::eigen::hcurl_null::GradientNullSpace;
use crate::eigen::pec_cavity::assemble_eigen_operator_p2;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// The shallower of the two section depths, as a fraction of the deeper
/// one ([`PortFaceProjection::guide_tm_guard`]).
pub const TM_GUIDE_SHALLOW_FRACTION: f64 = 2.0 / 3.0;

/// Lanczos steps [`share_free_tm_cutoff`] takes at most.
const MAX_LANCZOS_STEPS: usize = 240;

/// Relative residual `‖T x − μ x‖_K / μ` at which the largest Ritz pair of
/// [`share_free_tm_cutoff`] counts as converged.
const LANCZOS_TOL: f64 = 1e-9;

/// The share-free TM cutoff of one PEC-closed guide section
/// ([`share_free_tm_cutoff`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareFreeCutoff {
    /// `k_TM,h = (min ‖curl u‖² / ‖u·n‖²)^{1/2}` over the discretely
    /// divergence-free fields of the section (geometric, rad / mesh unit).
    pub k_tm: f64,
    /// The next stationary value of the same quotient (the second-largest
    /// Ritz value), `∞` if the Krylov space found only one.
    pub k_tm_next: f64,
    /// `‖u·n‖² / ‖u‖²` of the minimizer `u`.
    pub axial_share: f64,
    /// `(‖curl u‖² / ‖u‖²)^{1/2}` of the minimizer: its Rayleigh `k`.
    pub k_rayleigh: f64,
    /// Lanczos steps taken.
    pub steps: usize,
    /// Relative residual of the largest Ritz pair.
    pub residual: f64,
    /// Interior (PEC-reduced) H(curl) DOFs of the section.
    pub n_dofs: usize,
    /// Gradient constraints (columns of the discrete gradient).
    pub n_constraints: usize,
}

/// Why [`share_free_tm_cutoff`] could not compute a cutoff.
#[derive(Debug, Clone, PartialEq)]
pub enum ShareFreeError {
    /// The section has no interior DOF, or no tet.
    Empty,
    /// The section's topology admits loop harmonics (curl-free fields that
    /// are not gradients), so `K` is singular on the divergence-free
    /// fields and the quotient is unbounded below.
    LoopHarmonics,
    /// Assembly or factorization failed (the message).
    Solve(String),
    /// The Lanczos iteration did not converge (steps, residual).
    NotConverged {
        /// Steps taken.
        steps: usize,
        /// Relative residual reached.
        residual: f64,
    },
}

impl std::fmt::Display for ShareFreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the guide section has no interior DOF"),
            Self::LoopHarmonics => write!(
                f,
                "the guide section is not simply connected under its PEC closure (loop \
                 harmonics)"
            ),
            Self::Solve(e) => write!(f, "{e}"),
            Self::NotConverged { steps, residual } => write!(
                f,
                "Lanczos did not converge in {steps} steps (residual {residual:.2e})"
            ),
        }
    }
}

/// The share-free TM cutoff of `section`, closed with PEC on its whole
/// boundary, at p=2 (issue #955, option 3):
///
/// ```text
///   k_TM,h² = min ‖curl u‖² / ‖u·n‖²   over u ∈ V_h, Gᵀ M u = 0,
/// ```
///
/// with `V_h` the PEC-reduced p=2 Nédélec space, `G` its discrete gradient
/// (P2 Lagrange, [`GradientNullSpace`]), `M` the mass and `n` the unit
/// `normal`. Solved as the largest eigenvalue `μ = 1/k²` of
/// `M_n x = μ K x` on the divergence-free subspace, by Lanczos in the `K`
/// inner product on the operator `T x = y`,
///
/// ```text
///   [ K     M G ] [y]   [M_n x]
///   [ GᵀM   0   ] [p] = [  0  ],
/// ```
///
/// one sparse LU of the saddle-point matrix.
///
/// # Errors
///
/// [`ShareFreeError`].
pub fn share_free_tm_cutoff<B: Backend>(
    section: &TetMesh,
    normal: [f64; 3],
    device: &B::Device,
) -> Result<ShareFreeCutoff, ShareFreeError> {
    if section.n_tets() == 0 {
        return Err(ShareFreeError::Empty);
    }
    let space = HcurlSpace::build(section, ElementOrder::P2);
    let walls = section.boundary_faces();
    let mask = space
        .pec_interior_mask(section, &[&walls])
        .map_err(|e| ShareFreeError::Solve(e.to_string()))?;
    if !mask.iter().any(|&k| k) {
        return Err(ShareFreeError::Empty);
    }
    let null = GradientNullSpace::build(&space, section, &mask, &[]);
    if null.counts().loop_harmonics_bound > 0 {
        return Err(ShareFreeError::LoopHarmonics);
    }
    let n_t = section.n_tets();
    let one = vec![c64::new(1.0, 0.0); n_t];
    let nn: [[c64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| c64::new(normal[i] * normal[j], 0.0)));
    let eye: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let eps_n = vec![nn; n_t];
    let nu = vec![eye; n_t];
    let assemble = |m: DrivenMaterials<'_>| {
        assemble_eigen_operator_p2::<B>(&space, section, m, None, &mask, &[], device)
            .map_err(|e| ShareFreeError::Solve(format!("section assembly: {e}")))
    };
    let iso = assemble(DrivenMaterials::Scalar(&one))?;
    let axial = assemble(DrivenMaterials::MatchedUpml {
        epsilon_tensor: &eps_n,
        nu_tensor: &nu,
    })?;
    let n = iso.n_interior();
    // CSR-ish triplet views (the two operators share the interior pattern).
    let k_tr: Vec<(usize, usize, f64)> = iso
        .rows()
        .iter()
        .zip(iso.cols())
        .zip(iso.k_vals())
        .map(|((&r, &c), v)| (r, c, v.re))
        .collect();
    let m_tr: Vec<(usize, usize, f64)> = iso
        .rows()
        .iter()
        .zip(iso.cols())
        .zip(iso.m_vals())
        .map(|((&r, &c), v)| (r, c, v.re))
        .collect();
    let mn_tr: Vec<(usize, usize, f64)> = axial
        .rows()
        .iter()
        .zip(axial.cols())
        .zip(axial.m_vals())
        .map(|((&r, &c), v)| (r, c, v.re))
        .collect();
    let spmv = |tr: &[(usize, usize, f64)], x: &[f64]| {
        let mut y = vec![0.0; n];
        for &(r, c, v) in tr {
            y[r] += v * x[c];
        }
        y
    };

    // B = M G (n × m), column by column through M's rows.
    let m = null.dim();
    let mut m_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(r, c, v) in &m_tr {
        m_rows[c].push((r, v)); // M symmetric: column c of M.
    }
    let mut b_tr: Vec<(usize, usize, f64)> = Vec::new();
    let mut acc: std::collections::BTreeMap<usize, f64> = std::collections::BTreeMap::new();
    for j in 0..m {
        acc.clear();
        for &(r, g) in null.column(j) {
            for &(i, v) in &m_rows[r] {
                *acc.entry(i).or_insert(0.0) += v * g;
            }
        }
        b_tr.extend(acc.iter().map(|(&i, &v)| (i, j, v)));
    }
    // Balance the blocks so the LU pivots see comparable magnitudes; the
    // scale only rescales the multiplier `p`.
    let k_max = k_tr.iter().fold(0.0_f64, |a, t| a.max(t.2.abs()));
    let b_max = b_tr.iter().fold(0.0_f64, |a, t| a.max(t.2.abs()));
    let s = if b_max > 0.0 { k_max / b_max } else { 1.0 };
    let mut kkt: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(k_tr.len() + 2 * b_tr.len());
    kkt.extend(k_tr.iter().map(|&(r, c, v)| Triplet::new(r, c, v)));
    for &(i, j, v) in &b_tr {
        kkt.push(Triplet::new(i, n + j, s * v));
        kkt.push(Triplet::new(n + j, i, s * v));
    }
    let a = SparseColMat::<usize, f64>::try_new_from_triplets(n + m, n + m, &kkt)
        .map_err(|e| ShareFreeError::Solve(format!("saddle-point assembly: {e:?}")))?;
    let lu = a
        .as_ref()
        .sp_lu()
        .map_err(|e| ShareFreeError::Solve(format!("saddle-point LU: {e:?}")))?;
    let apply_t = |x: &[f64]| -> Vec<f64> {
        let rhs = spmv(&mn_tr, x);
        let mut b = Mat::<f64>::from_fn(n + m, 1, |i, _| if i < n { rhs[i] } else { 0.0 });
        lu.solve_in_place(b.as_mut());
        (0..n).map(|i| b[(i, 0)]).collect()
    };
    let dot = |x: &[f64], y: &[f64]| x.iter().zip(y).map(|(a, b)| a * b).sum::<f64>();

    // Lanczos in the K inner product, full reorthogonalization. T is
    // K-self-adjoint and positive semidefinite on the divergence-free
    // fields, and every T x is divergence-free.
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let r0: Vec<f64> = (0..n)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        })
        .collect();
    let mut q = apply_t(&r0);
    let mut kq = spmv(&k_tr, &q);
    let nrm = dot(&q, &kq).sqrt();
    if !(nrm.is_finite() && nrm > 0.0) {
        return Err(ShareFreeError::Solve(
            "the start vector has no divergence-free component".into(),
        ));
    }
    q.iter_mut().for_each(|v| *v /= nrm);
    kq.iter_mut().for_each(|v| *v /= nrm);
    let mut qs: Vec<Vec<f64>> = vec![q];
    let mut kqs: Vec<Vec<f64>> = vec![kq];
    let (mut alpha, mut beta): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let max_steps = MAX_LANCZOS_STEPS.min(n.saturating_sub(m).max(1));
    let mut last: Option<(f64, f64, f64, Vec<f64>)> = None;
    for j in 0..max_steps {
        let mut w = apply_t(&qs[j]);
        alpha.push(dot(&kqs[j], &w));
        for _ in 0..2 {
            for (qi, kqi) in qs.iter().zip(&kqs) {
                let h = dot(kqi, &w);
                w.iter_mut().zip(qi).for_each(|(wv, qv)| *wv -= h * qv);
            }
        }
        let kw = spmv(&k_tr, &w);
        let b_j = dot(&w, &kw).max(0.0).sqrt();
        let steps = j + 1;
        let check = steps % 5 == 0 || steps == max_steps || b_j <= 1e-14 * alpha[0].abs();
        if check {
            let (mus, s_mat) = crate::eigen::lanczos::tridiag_eigenpairs(&alpha, &beta)
                .map_err(|e| ShareFreeError::Solve(e.to_string()))?;
            let top = mus.len() - 1;
            let mu = mus[top];
            let next = if top > 0 { mus[top - 1] } else { 0.0 };
            let resid = b_j * s_mat[(steps - 1, top)].abs() / mu;
            let s_col: Vec<f64> = (0..steps).map(|i| s_mat[(i, top)]).collect();
            last = Some((mu, next, resid, s_col));
            if resid <= LANCZOS_TOL || b_j <= 1e-14 * alpha[0].abs() {
                break;
            }
        }
        if steps == max_steps {
            break;
        }
        beta.push(b_j);
        let inv = 1.0 / b_j;
        qs.push(w.iter().map(|v| v * inv).collect());
        kqs.push(kw.iter().map(|v| v * inv).collect());
    }
    let (mu, next, resid, s_col) = last.expect("at least one check");
    let steps = s_col.len();
    if resid > 1e-6 {
        return Err(ShareFreeError::NotConverged {
            steps,
            residual: resid,
        });
    }
    // The minimizer u = Q s and its quadratic forms.
    let mut u = vec![0.0; n];
    for (qi, &si) in qs.iter().zip(&s_col) {
        u.iter_mut().zip(qi).for_each(|(uv, qv)| *uv += si * qv);
    }
    let uk = dot(&u, &spmv(&k_tr, &u));
    let um = dot(&u, &spmv(&m_tr, &u));
    let un = dot(&u, &spmv(&mn_tr, &u));
    Ok(ShareFreeCutoff {
        k_tm: (1.0 / mu).sqrt(),
        k_tm_next: if next > 0.0 {
            (1.0 / next).sqrt()
        } else {
            f64::INFINITY
        },
        axial_share: un / um,
        k_rayleigh: (uk / um).sqrt(),
        steps,
        residual: resid,
        n_dofs: n,
        n_constraints: m,
    })
}

/// `|E·n|²` share at and above which a mode counts as TM-like in the
/// measurement tables of issue #905 (`tests/wave_port_p2.rs`). The
/// share-free cutoff does not use it; the guard's floor
/// ([`GuideTmGuard::guard_k_c`]) does: no resonance of the section with at
/// least this exact share lies below `√share·k_TM,h` (module docs).
pub const TM_LIKE_AXIAL_SHARE: f64 = 0.4;

/// Where the TM cutoff of a [`GuideTmGuard`] comes from (issue #955).
#[derive(Debug, Clone, PartialEq)]
pub enum TmCutoffSource {
    /// The share-free cutoff of the PEC-closed guide section
    /// ([`share_free_tm_cutoff`]) at two depths.
    ShareFree3d {
        /// `min(k_deep, k_shallow)` (geometric, rad / mesh unit).
        k_c: f64,
        /// The deeper section's depth (mesh units, either side of the
        /// port plane).
        depth: f64,
        /// The deeper section's cutoff.
        k_deep: f64,
        /// The shallower section's depth.
        depth_shallow: f64,
        /// The shallower section's cutoff (`k_deep` when both depths select
        /// the same tets and one solve served both).
        k_shallow: f64,
        /// `|E·n|²` share of the minimizer at `k_c`.
        axial_share: f64,
    },
    /// The interim margin law ([`tm_guard_margin`](super::tm_guard_margin)),
    /// because the computation does not apply.
    MarginLaw {
        /// Why.
        reason: TmMarginLawReason,
    },
}

/// Why a [`GuideTmGuard`] fell back to the interim margin law.
#[derive(Debug, Clone, PartialEq)]
pub enum TmMarginLawReason {
    /// The estimate guards a p=1 solve: p=1 keeps its law, bit for bit.
    ElementOrderP1,
    /// Part of the face rim is open (not on a conductor wall), so closing
    /// the section's lateral boundary with PEC would model another guide.
    OpenRim,
    /// The face estimate has no finite TM cutoff.
    NoFaceCutoff,
    /// No tet of the mesh lies over the port face.
    EmptySection,
    /// The share-free solve failed.
    Solve(ShareFreeError),
}

/// The TM guard of a p=2 wave port from the share-free cutoff of its guide
/// section ([`PortFaceProjection::guide_tm_guard`], issue #955).
#[derive(Debug, Clone, PartialEq)]
pub struct GuideTmGuard {
    /// Where the cutoff comes from.
    pub source: TmCutoffSource,
    /// The geometric TM cutoff the guard rejects at (rad / mesh unit; scale
    /// by `1/√(Re ε_n·μ_t)` for a filled guide): the floor
    /// `√`[`TM_LIKE_AXIAL_SHARE`]` · k_c` for a share-free source, the
    /// interim guard on a fallback.
    pub guard_k_c: f64,
    /// The interim guard on the same estimate
    /// ([`TmCutoffEstimate::guard_k_c`]), for comparison.
    pub margin_law_guard_k_c: f64,
}

impl GuideTmGuard {
    /// `true` when the guard comes from the share-free cutoff (not the
    /// margin law).
    pub fn is_computed(&self) -> bool {
        matches!(self.source, TmCutoffSource::ShareFree3d { .. })
    }

    /// The guard's relative margin below the face estimate `k_face`:
    /// `1 − guard_k_c / k_face`.
    pub fn margin_below(&self, k_face: f64) -> f64 {
        1.0 - self.guard_k_c / k_face
    }
}

impl PortFaceProjection {
    /// The TM guard of a **p=2** wave port from the share-free cutoff of
    /// its guide section (issue #955, option 3; module docs).
    ///
    /// `estimate` is the face estimate with its axial spacing set
    /// ([`PortFaceProjection::tm_cutoff_estimate_at_order`] at p=2 and
    /// [`TmCutoffEstimate::with_axial_spacing`]); it supplies the interim
    /// guard of the fallbacks and of [`GuideTmGuard::margin_law_guard_k_c`].
    /// `open_rim` is as in
    /// [`PortFaceProjection::lowest_tm_cutoff`]. `reach` is the deeper
    /// section's depth either side of the port plane, as for
    /// [`PortFaceProjection::guide_axial_spacing`]: use
    /// [`super::tm_guard_axial_reach`]. A depth beyond the guide's own
    /// extent over the face is clipped to it.
    ///
    /// For a p=1 estimate this returns the interim guard unchanged
    /// ([`TmMarginLawReason::ElementOrderP1`]) without touching the mesh.
    ///
    /// # Panics
    ///
    /// Panics if `open_rim` is given with a length other than
    /// `self.edges.len()`.
    pub fn guide_tm_guard<B: Backend>(
        &self,
        mesh: &TetMesh,
        estimate: &TmCutoffEstimate,
        open_rim: Option<&[bool]>,
        reach: f64,
        device: &B::Device,
    ) -> GuideTmGuard {
        let interim = estimate.guard_k_c();
        let fallback = |reason| GuideTmGuard {
            source: TmCutoffSource::MarginLaw { reason },
            guard_k_c: interim,
            margin_law_guard_k_c: interim,
        };
        if estimate.element_order != ElementOrder::P2 {
            return fallback(TmMarginLawReason::ElementOrderP1);
        }
        if let Some(o) = open_rim {
            assert_eq!(
                o.len(),
                self.edges.len(),
                "open_rim must be aligned with the port-face edges"
            );
            if o.iter()
                .zip(&self.interior_edge_mask)
                .any(|(&open, &interior)| open && !interior)
            {
                return fallback(TmMarginLawReason::OpenRim);
            }
        }
        let k_face = estimate.k_c();
        if !(k_face.is_finite() && k_face > 0.0) {
            return fallback(TmMarginLawReason::NoFaceCutoff);
        }
        let footprint = self.guide_footprint(mesh);
        if footprint.is_empty() {
            return fallback(TmMarginLawReason::EmptySection);
        }
        let span = footprint.iter().fold(0.0_f64, |m, &(_, _, d)| m.max(d));
        let depth = reach.max(0.0).min(span);
        let depth_shallow = TM_GUIDE_SHALLOW_FRACTION * depth;
        let select = |dmax: f64| -> Vec<usize> {
            footprint
                .iter()
                .filter(|&&(_, _, d)| d <= dmax)
                .map(|&(t, _, _)| t)
                .collect()
        };
        let (deep, shallow) = (select(depth), select(depth_shallow));
        let solve = |tets: &[usize]| {
            share_free_tm_cutoff::<B>(&section_mesh(mesh, tets), self.normal, device)
        };
        let c_deep = match solve(&deep) {
            Ok(c) => c,
            Err(e) => return fallback(TmMarginLawReason::Solve(e)),
        };
        let c_shallow = if shallow == deep {
            c_deep
        } else {
            match solve(&shallow) {
                Ok(c) => c,
                Err(e) => return fallback(TmMarginLawReason::Solve(e)),
            }
        };
        let low = if c_shallow.k_tm < c_deep.k_tm {
            c_shallow
        } else {
            c_deep
        };
        let k_c = low.k_tm;
        GuideTmGuard {
            source: TmCutoffSource::ShareFree3d {
                k_c,
                depth,
                k_deep: c_deep.k_tm,
                depth_shallow,
                k_shallow: c_shallow.k_tm,
                axial_share: low.axial_share,
            },
            guard_k_c: TM_LIKE_AXIAL_SHARE.sqrt() * k_c,
            margin_law_guard_k_c: interim,
        }
    }

    /// The sub-mesh of the guide section within `depth` of the port plane
    /// (either side): the tets of the guide footprint whose nearest point
    /// to the plane is within `depth` (the window of
    /// [`Self::guide_axial_spacing`]). Physical groups are not carried.
    pub fn guide_section(&self, mesh: &TetMesh, depth: f64) -> TetMesh {
        let tets: Vec<usize> = self
            .guide_footprint(mesh)
            .into_iter()
            .filter(|&(_, _, d)| d <= depth)
            .map(|(t, _, _)| t)
            .collect();
        section_mesh(mesh, &tets)
    }
}

/// The sub-mesh of `mesh` made of the tets `tets` (indices into
/// `mesh.tets`, in that order), with its nodes renumbered in ascending
/// original order.
fn section_mesh(mesh: &TetMesh, tets: &[usize]) -> TetMesh {
    let mut used = vec![false; mesh.nodes.len()];
    for &t in tets {
        for &n in &mesh.tets[t] {
            used[n as usize] = true;
        }
    }
    let mut map = vec![u32::MAX; mesh.nodes.len()];
    let mut nodes = Vec::new();
    for (i, &u) in used.iter().enumerate() {
        if u {
            map[i] = nodes.len() as u32;
            nodes.push(mesh.nodes[i]);
        }
    }
    TetMesh {
        nodes,
        tets: tets
            .iter()
            .map(|&t| mesh.tets[t].map(|n| map[n as usize]))
            .collect(),
        physical_groups: Default::default(),
    }
}
