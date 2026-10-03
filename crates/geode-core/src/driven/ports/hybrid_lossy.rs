//! **Lossy / dispersive hybrid wave ports** (Epic #778 Phase 4, issue #806):
//! the per-frequency port state of a [`HybridWavePort`] whose face carries a
//! complex `ε` ([`HybridPortFace::is_lossy`](super::HybridPortFace::is_lossy)),
//! or that sits in a dispersive sweep
//! ([`super::solve_mixed_port_spec_sweep_dispersive_with_mode`]).
//!
//! The modes come from the complex-symmetric pencil
//! ([`crate::analytic::lossy_port_modes`]), re-solved at every `ω` on the
//! face `ε(ω)`. Everything else follows Phase 2 ([`super::hybrid`]), with the
//! real quantities replaced by their unconjugated complex counterparts:
//!
//! - **Flux.** `w̃ = ẽ_t + Dẽ_z`, `N = ẽ_tᵀM₁w̃` (complex, `= β²` by the
//!   normalization), `κ = 1/√N`, `f̂ = S_p κw̃`, so `f̂ᵀE_t = 1`, and the
//!   modal admittance factor is the complex `β`. The rank-N SMW, the drive,
//!   the readout and the `√β` power weights are the mixed path's, unchanged;
//!   `A(ω)ᵀ = A(ω)` and `Sᵀ = S` hold. With loss the `√β` weights are
//!   complex and S is pseudo-power-normalized: passivity is measured, not
//!   guaranteed (module docs of [`crate::analytic::lossy_port_modes`]).
//! - **Tracking.** Greedy one-to-one on `|ẽ_{t,prev}ᵀM₁w̃_new| /
//!   √(|N_prev||N_new|)`; the sign is fixed so that `Re(ẽ_{t,prev}ᵀM₁w̃_new /
//!   N_new) > 0` (`≈ +1` for the same mode, since both members are
//!   normalized to their own `β²`). The same loud "mode identity lost"
//!   errors as Phase 2. **Exactly degenerate clusters** (a homogeneous lossy
//!   multi-conductor face; #817) are tracked as subspaces exactly as on the
//!   real path ([`super::hybrid`] module docs), with the rotations
//!   **complex orthogonal** (`RᵀR = I`, which keeps the unconjugated
//!   B-normalization `zᵀBz = β²`): the Procrustes factor is
//!   `Oᵀ(OOᵀ)^{-1/2}` with the complex-symmetric inverse square root
//!   (Denman–Beavers), the principal cosines are the singular values of `O`,
//!   and the canonical first-frequency basis diagonalizes the complex
//!   symmetric `QQᵀ` of the conductor currents (complex-orthogonal
//!   eigenvectors for distinct eigenvalues).
//! - **Certificates.** As on the real path: an uncertified multiplicity
//!   pass is retried with twice the Krylov cap and then warned about; each
//!   channel reports its residual floor and floor acceptance. Line
//!   impedances are complex on a lossy face and reported as
//!   [`super::HybridComplexLineReport`] (`line_lossy`; `line` stays `None`):
//!   the unconjugated `Z_PI = 2P/Σ I_c²` etc. of Phase 5's definition (#807).
//! - **Completeness.** Every face mode with `Re β² > 0` must be a reported
//!   channel.
//! - **Termination.** The next evanescent modes, one slot each. A
//!   mesh-collided LSE/LSM pair is two complex modes here (no conjugate
//!   symmetry), each terminated on its own; in the lossless limit that is the
//!   Phase 2 2×2 block. A mode whose self-pairing is degenerate (`zᵀBz ≈ 0`)
//!   stops the termination window with a
//!   [`PortWarningKind::ComplexPairDropped`] warning, and is an error in a
//!   reported slot.
//! - **Accuracy.** The `h/2` (and, at the first frequency, `h/4`) re-solve
//!   of the lossy face ([`LossyRefinement`]) gives the per-mode `β` estimate
//!   and an `α` estimate. A refined solve that fails degrades the estimate to
//!   [`PortWarningKind::AccuracyUnavailable`] rather than aborting the sweep
//!   (the #815 review's first follow-up note, applied to this path). With
//!   [`PortAccuracyOpts::alpha_threshold`] set, an `α` estimate above it
//!   raises [`PortWarningKind::AttenuationAccuracyAboveThreshold`] (the #819
//!   review's note; the `geode` CLI sets it to the `β` threshold).
//!
//! Touchstone (#775) renormalizes with a characteristic impedance; a lossy
//! hybrid channel's is complex and frequency dependent, which the Γ-form
//! renormalization to a real `reference_ohm` accepts. Phase 5 (#807) uses the
//! complex `Z_PI` of [`super::HybridComplexLineReport`] by default.

use faer::c64;

use super::hybrid::{
    ChanAt, FaceCtx, HybridChannelReport, HybridComplexLineReport, HybridModalFlux,
    HybridPortPointReport, HybridPortReport, HybridWavePort, PortAccuracyOpts, PortWarning,
    PortWarningKind, check_lossy_eps, cluster_warning, multiplicity_warning, sym_eig,
};
use super::hybrid_z::{
    ImpedanceAccuracy, LineGeom, LineLevel, ZOutcome, ZWarnings, impedance_accuracy, line_complex,
    line_z_c, observed_rates,
};
use super::wave::invert_complex_dense;
use crate::analytic::lossy_port_modes::{
    LossyHybridMode, LossyHybridModeSet, LossyRefinement, dot_u, lossy_alpha_estimate,
    lossy_mode_accuracy, lossy_pairing_vector, match_refined_lossy_modes, rmatvec,
    solve_lossy_hybrid_port_modes,
};
use crate::analytic::port_mode_accuracy::{ModeAccuracy, NOMINAL_RATE, observed_rate};
use crate::analytic::port_modes::{DEGENERATE_REL_TOL, HybridPortError, HybridPortOpts};
use crate::driven::solve::DrivenError;
use crate::mesh::TetMesh;

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

/// Per-port state of a lossy hybrid port over a sweep.
pub(super) struct LossyState {
    ctx: FaceCtx,
    /// Tracked reported modes at the previous frequency (sign-fixed).
    prev: Vec<LossyHybridMode>,
    refined: Option<LossyRefinement>,
    refined2: Option<LossyRefinement>,
    rates: Option<Vec<f64>>,
    /// Line operators of the `h/2` face (`Some(None)`: they failed to
    /// assemble) for the impedance estimate.
    z_h2: Option<Option<LineGeom>>,
    /// Observed per-channel impedance rates at the first frequency.
    z_rates: Option<Vec<[Option<f64>; 3]>>,
    z_warn: ZWarnings,
    points: Vec<Option<HybridPortPointReport>>,
    dropped_warn: Vec<PortWarningKind>,
    acc_warn: Vec<Option<PortWarningKind>>,
    /// Worst [`PortWarningKind::AttenuationAccuracyAboveThreshold`] per channel.
    alpha_warn: Vec<Option<PortWarningKind>>,
    acc_unavail: Vec<Option<PortWarningKind>>,
    /// `k₀²ε′_max` at the previous frequency.
    prev_scale: f64,
    /// `(first ω, count, verified)` of uncertified multiplicity solves.
    mult_warn: Option<(f64, usize, bool)>,
    /// Cluster splits and non-canonical cluster bases.
    cluster_warn: Vec<PortWarningKind>,
}

/// The face `ε` at this frequency: from the dispersive volume vector through
/// the face's tet map, or the face's own complex `ε`.
fn face_eps(
    port: &HybridWavePort,
    eps_tet: Option<&[c64]>,
    p_idx: usize,
    omega: f64,
) -> Result<Vec<c64>, DrivenError> {
    let face = &port.face;
    match eps_tet {
        Some(vol) => {
            let tets = face
                .tet_of_tri
                .as_ref()
                .expect("dispersive sweeps check the tet map at setup");
            let e: Vec<c64> = tets.iter().map(|&t| vol[t]).collect();
            check_lossy_eps(&e, "face triangle").map_err(|err| DrivenError::InvalidPort {
                index: p_idx,
                reason: format!("hybrid wave port: dispersive ε(ω) at ω = {omega}: {err}"),
            })?;
            Ok(e)
        }
        None => Ok(face
            .eps_c
            .clone()
            .unwrap_or_else(|| face.eps_r.iter().map(|&e| c64::new(e, 0.0)).collect())),
    }
}

/// Mode solve at `omega` with `extra` evanescent slots beyond the reported
/// and termination ones; one retry with the bare minimum on a shortfall (as
/// Phase 2), and one retry with twice the Krylov cap when the multiplicity
/// pass could not certify (#817). Returns the set and whether that retry ran.
fn solve(
    port: &HybridWavePort,
    eps: &[c64],
    omega: f64,
    extra: usize,
) -> Result<(LossyHybridModeSet, bool), DrivenError> {
    let base = port.n_modes() + port.opts.n_termination_evanescent;
    let mut opts = HybridPortOpts {
        n_evanescent: base + extra,
        max_krylov: port.opts.max_krylov,
        residual_tol: port.opts.residual_tol,
        verify_multiplicity: port.opts.verify_multiplicity,
        ..HybridPortOpts::default()
    };
    let f = &port.face;
    let run = |o: &mut HybridPortOpts| {
        let go = |o: &HybridPortOpts| {
            solve_lossy_hybrid_port_modes(
                &f.projection.tri_mesh,
                eps,
                &f.interior_edge_mask,
                &f.free_node_mask,
                omega,
                o,
            )
        };
        match go(o) {
            Ok(s) => Ok(s),
            Err(HybridPortError::Shortfall { n_propagating, .. }) if extra > 0 => {
                o.n_evanescent = base.saturating_sub(n_propagating);
                go(o)
            }
            Err(e) => Err(e),
        }
    };
    let set = run(&mut opts).map_err(|e| {
        DrivenError::Solve(format!(
            "hybrid wave port: lossy port-mode solve at ω = {omega} failed: {e}"
        ))
    })?;
    if opts.verify_multiplicity && !set.diagnostics.multiplicity_certified {
        opts.max_krylov = 2 * opts.max_krylov.max(1);
        opts.n_evanescent = base + extra;
        return Ok((run(&mut opts).unwrap_or(set), true));
    }
    Ok((set, false))
}

/// Flux (and lifted-to-local `E_t`) of one lossy channel.
fn channel(ctx: &FaceCtx, m: &LossyHybridMode, tripwire: bool) -> (Vec<(usize, c64)>, Vec<c64>) {
    let mut m1v = vec![ZERO; m.e_t.len()];
    if tripwire {
        rmatvec(ctx.m1.as_ref(), &m.e_t, &mut m1v);
        let k = dot_u(&m.e_t, &m1v).sqrt().inv();
        let e: Vec<c64> = m.e_t.iter().map(|&v| k * v).collect();
        return (ctx.flux(&e), e);
    }
    let w = lossy_pairing_vector(&ctx.d, m);
    rmatvec(ctx.m1.as_ref(), &w, &mut m1v);
    // N = ẽ_tᵀM₁w̃ (= β² by the normalization; recomputed).
    let kappa = dot_u(&m.e_t, &m1v).sqrt().inv();
    let w_c: Vec<c64> = w.iter().map(|&v| kappa * v).collect();
    let e: Vec<c64> = m.e_t.iter().map(|&v| kappa * v).collect();
    (ctx.flux(&w_c), e)
}

impl LossyState {
    pub(super) fn new(ctx: FaceCtx, port: &HybridWavePort, n_omegas: usize) -> Self {
        Self {
            ctx,
            prev: Vec::new(),
            refined: None,
            refined2: None,
            rates: None,
            z_h2: None,
            z_rates: None,
            z_warn: ZWarnings::new(port.n_modes()),
            points: vec![None; n_omegas],
            dropped_warn: Vec::new(),
            acc_warn: vec![None; port.n_modes()],
            alpha_warn: vec![None; port.n_modes()],
            acc_unavail: vec![None; port.n_modes()],
            prev_scale: 0.0,
            mult_warn: None,
            cluster_warn: Vec::new(),
        }
    }

    /// Solve, track and lift one lossy hybrid port at `omega`; returns the
    /// reported and the termination channels. `eps_tet`: the dispersive
    /// volume's `ε(ω)` (`None` in a fixed-material sweep).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn channels_at(
        &mut self,
        port: &HybridWavePort,
        p_idx: usize,
        omega: f64,
        input_index: usize,
        first: bool,
        eps_tet: Option<&[c64]>,
    ) -> Result<(Vec<ChanAt>, Vec<ChanAt>), DrivenError> {
        let k = port.n_modes();
        let eps = face_eps(port, eps_tet, p_idx, omega)?;
        let (set, retried) = solve(port, &eps, omega, 2)?;
        let n_prop = set.n_propagating;
        let scale = omega * omega * self.ctx.eps_max;
        if !set.diagnostics.multiplicity_certified {
            match &mut self.mult_warn {
                Some(w) => w.1 += 1,
                None => self.mult_warn = Some((omega, 1, port.opts.verify_multiplicity)),
            }
        }

        // Units: exactly degenerate clusters (#817).
        let unit_members = lossy_units(&set.modes, scale);
        let mut unit_of = vec![0usize; set.modes.len()];
        for (u, mem) in unit_members.iter().enumerate() {
            for &j in mem {
                unit_of[j] = u;
            }
        }
        let mut units: Vec<LUnit> = unit_members.into_iter().map(LUnit::identity).collect();
        let mut overlaps: Vec<Option<f64>> = vec![None; k];

        if first || self.prev.is_empty() {
            let mut taken = 0usize;
            for unit in units.iter_mut() {
                if taken >= k {
                    break;
                }
                let g = unit.members.len();
                if taken + g > k {
                    return Err(DrivenError::InvalidPort {
                        index: p_idx,
                        reason: format!(
                            "hybrid wave port: at ω = {omega} the last {} of the {k} reported \
                             channel(s) would split an exactly degenerate cluster of {g} lossy \
                             modes (β² = {}); a channel inside a degenerate eigenspace is not \
                             uniquely defined — report {taken} or {} channel(s)",
                            k - taken,
                            set.modes[unit.members[0]].beta_sq,
                            taken + g
                        ),
                    });
                }
                unit.claimed = (0..g).map(|d| (d, taken + d)).collect();
                if g > 1 {
                    self.canonicalize(&set.modes, unit, omega);
                }
                taken += g;
            }
            if taken < k {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: only {} modes at ω = {omega}, {k} channels \
                     requested",
                    set.modes.len()
                )));
            }
        } else {
            self.track(
                &set,
                &mut units,
                &mut overlaps,
                p_idx,
                omega,
                port.opts.min_track_overlap,
            )?;
        }
        for u in &units {
            if u.members.len() == 1
                && let Some(&(_, pos)) = u.claimed.first()
                && set.modes[u.members[0]].degenerate()
            {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "hybrid wave port: reported channel {pos} at ω = {omega} has a degenerate \
                         self-pairing zᵀBz ≈ 0 (β² = {}) and cannot be normalized; report fewer \
                         channels or refine the port face (h = {:.4e})",
                        set.modes[u.members[0]].beta_sq, self.ctx.h
                    ),
                });
            }
        }

        // Completeness: every propagating (Re β² > 0) mode is a reported
        // channel (a cluster counts once all its directions are claimed).
        for (i, m) in set.modes.iter().enumerate().take(n_prop) {
            let u = &units[unit_of[i]];
            if u.claimed.len() < u.members.len() {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "hybrid wave port: at ω = {omega} the lossy port face carries {n_prop} \
                         propagating mode(s) (Re β > |Im β|) but only {k} reported channel(s); \
                         the mode with β = {} (E_z energy fraction {:.2}) would be unterminated \
                         and S silently wrong — raise the port's mode count",
                        m.beta,
                        1.0 - m.transverse_fraction
                    ),
                });
            }
        }

        let mut tracked_opt: Vec<Option<(LossyHybridMode, usize)>> = vec![None; k];
        for u in &units {
            for &(dir, ch) in &u.claimed {
                tracked_opt[ch] = Some((u.direction(&set.modes, dir), u.members.len()));
            }
        }
        let (tracked, cluster_sizes): (Vec<LossyHybridMode>, Vec<usize>) = tracked_opt
            .into_iter()
            .map(|t| t.expect("every channel assigned"))
            .unzip();
        let mut reported = Vec::with_capacity(k);
        for (m, &a) in tracked.iter().zip(&port.a_inc) {
            let (flux, _) = channel(&self.ctx, m, port.opts.transverse_only_flux);
            reported.push(ChanAt {
                beta: m.beta,
                y: m.beta,
                a_inc: Some(a),
                flux,
            });
        }

        // Termination window: the unclaimed directions of each unit, one
        // slot each.
        let want = port.opts.n_termination_evanescent;
        let mut termination = Vec::new();
        'units: for u in &units {
            for dir in u.claimed.len()..u.members.len() {
                if termination.len() >= want {
                    break 'units;
                }
                let m = u.direction(&set.modes, dir);
                if m.degenerate() {
                    self.dropped_warn.push(PortWarningKind::ComplexPairDropped {
                        mode_indices: [u.members[0], u.members[0]],
                        omega,
                        unfilled_slots: want - termination.len(),
                    });
                    break 'units;
                }
                let (flux, _) = channel(&self.ctx, &m, port.opts.transverse_only_flux);
                termination.push(ChanAt {
                    beta: m.beta,
                    y: m.beta,
                    a_inc: None,
                    flux,
                });
            }
        }
        let term_real = termination.len();

        // Line quantities and the accuracy estimate.
        let lines: Vec<Option<HybridComplexLineReport>> = tracked
            .iter()
            .map(|m| lossy_line(&self.ctx, port, m, omega, &eps))
            .collect();
        let mut accuracy: Vec<Option<ModeAccuracy>> = vec![None; k];
        let mut z_acc: Vec<Option<ImpedanceAccuracy>> = vec![None; k];
        if let Some(acc) = port.opts.accuracy {
            let z_out;
            (accuracy, z_out) = self.estimate(
                port,
                &tracked,
                &lines,
                &cluster_sizes,
                &eps,
                omega,
                first,
                acc,
            );
            for (c, z) in z_out.iter().enumerate() {
                self.z_warn
                    .record(c, omega, z, &acc, self.ctx.h, self.ctx.h_min);
                z_acc[c] = z.accuracy();
            }
            for (c, a) in accuracy.iter().enumerate() {
                match a {
                    None => {
                        if self.acc_unavail[c].is_none() {
                            self.acc_unavail[c] =
                                Some(PortWarningKind::AccuracyUnavailable { channel: c, omega });
                        }
                    }
                    Some(a) if tracked[c].is_propagating() && a.estimate > acc.threshold => {
                        let worse = match &self.acc_warn[c] {
                            Some(PortWarningKind::AccuracyAboveThreshold { estimate, .. }) => {
                                a.estimate > *estimate
                            }
                            _ => true,
                        };
                        if worse {
                            self.acc_warn[c] = Some(PortWarningKind::AccuracyAboveThreshold {
                                channel: c,
                                omega,
                                estimate: a.estimate,
                                threshold: acc.threshold,
                                h: self.ctx.h,
                                h_required: a.required_h(self.ctx.h, acc.threshold),
                                ez_energy_fraction: a.ez_energy_fraction,
                            });
                        }
                    }
                    Some(_) => {}
                }
                // The attenuation estimate (#819 review), opt-in.
                if let (Some(a), Some(thr)) = (a, acc.alpha_threshold)
                    && tracked[c].is_propagating()
                    && let Some(est) = lossy_alpha_estimate(a)
                    && est > thr
                {
                    let worse = match &self.alpha_warn[c] {
                        Some(PortWarningKind::AttenuationAccuracyAboveThreshold {
                            estimate,
                            ..
                        }) => est > *estimate,
                        _ => true,
                    };
                    if worse {
                        let h = self.ctx.h;
                        self.alpha_warn[c] =
                            Some(PortWarningKind::AttenuationAccuracyAboveThreshold {
                                channel: c,
                                omega,
                                estimate: est,
                                threshold: thr,
                                h,
                                h_required: h * (thr / est).powf(1.0 / a.rate),
                            });
                    }
                }
            }
        }

        let tol = port.opts.residual_tol;
        self.points[input_index] = Some(HybridPortPointReport {
            omega,
            channels: tracked
                .iter()
                .zip(&overlaps)
                .zip(accuracy)
                .zip(&cluster_sizes)
                .zip(lines.into_iter().zip(z_acc))
                .map(
                    |((((m, &ov), a), &g), (line_lossy, za))| HybridChannelReport {
                        beta: m.beta,
                        beta_sq: m.beta_sq.re,
                        beta_sq_im: m.beta_sq.im,
                        ez_energy_fraction: 1.0 - m.transverse_fraction,
                        track_overlap: ov,
                        alpha_accuracy: a.as_ref().and_then(lossy_alpha_estimate),
                        accuracy: a,
                        impedance_accuracy: za,
                        residual: m.residual,
                        residual_floor: m.residual_floor,
                        floor_accepted: m.residual > tol && m.residual <= m.residual_floor,
                        cluster_size: g,
                        // A lossy line's impedance is complex: `line_lossy`.
                        line: None,
                        line_lossy,
                    },
                )
                .collect(),
            n_propagating: n_prop,
            termination_real: term_real,
            termination_pairs: 0,
            multiplicity_certified: set.diagnostics.multiplicity_certified,
            multiplicity_retried: retried,
            repeated_copies: set.diagnostics.repeated_copies,
            degenerate_clusters: set.diagnostics.degenerate_clusters,
            floor_accepted: set.diagnostics.floor_accepted,
        });
        self.prev = tracked;
        self.prev_scale = scale;
        Ok((reported, termination))
    }

    /// Canonical basis of a reported degenerate lossy cluster at the first
    /// frequency: the complex-orthogonal eigenvectors of the complex
    /// symmetric `QQᵀ` (`Q` the `g × n_c` conductor currents), ordered by
    /// descending `|λ|`, each signed so its first conductor carrying at least
    /// half the largest current has `Re I > 0` (the real path's rule; in the
    /// lossless limit the same basis). Kept as the solver's basis, with a
    /// warning, when the conductors cannot separate the modes.
    fn canonicalize(&mut self, modes: &[LossyHybridMode], unit: &mut LUnit, omega: f64) {
        let g = unit.members.len();
        let channels: Vec<usize> = unit.claimed.iter().map(|&(_, c)| c).collect();
        let n_c = self.ctx.conductors.len();
        let mut fail = |reason: String| {
            self.cluster_warn
                .push(PortWarningKind::NonCanonicalClusterBasis {
                    channels: channels.clone(),
                    omega,
                    reason,
                });
        };
        if n_c < g {
            fail(format!(
                "{g} degenerate modes but {n_c} floating conductor(s)"
            ));
            return;
        }
        let q: Vec<Vec<c64>> = unit
            .members
            .iter()
            .map(|&j| self.ctx.currents_c(&modes[j].e_t, &modes[j].e_z, omega))
            .collect();
        let a = faer::Mat::<c64>::from_fn(g, g, |r, c| {
            (0..n_c).fold(ZERO, |acc, k| acc + q[r][k] * q[c][k])
        });
        let Ok(eig) = a.eigen() else {
            fail("the current-matrix eigensolve failed".into());
            return;
        };
        let lam: Vec<c64> = eig.S().column_vector().iter().copied().collect();
        let u = eig.U();
        let mut order: Vec<usize> = (0..g).collect();
        order.sort_by(|&x, &y| lam[y].norm().total_cmp(&lam[x].norm()));
        let top = lam[order[0]].norm().max(f64::MIN_POSITIVE);
        let mut gap = f64::INFINITY;
        for x in 0..g {
            for y in x + 1..g {
                gap = gap.min((lam[x] - lam[y]).norm() / top);
            }
        }
        if lam[order[g - 1]].norm() <= 1e-12 * top || gap <= 1e-6 {
            fail(format!(
                "conductor currents do not separate the modes (relative eigenvalue gap \
                 {gap:.1e})"
            ));
            return;
        }
        let mut rot = vec![vec![ZERO; g]; g];
        for (kcol, &e) in order.iter().enumerate() {
            let col: Vec<c64> = (0..g).map(|r| u[(r, e)]).collect();
            let nn = col.iter().fold(ZERO, |acc, v| acc + v * v);
            let scale2: f64 = col.iter().map(|v| v.norm_sqr()).sum();
            if nn.norm() <= 1e-6 * scale2 {
                fail("an isotropic (zᵀz ≈ 0) current eigenvector".into());
                return;
            }
            let inv = nn.sqrt().inv();
            for r in 0..g {
                rot[r][kcol] = col[r] * inv;
            }
        }
        for kcol in 0..g {
            let cur: Vec<c64> = (0..n_c)
                .map(|c| (0..g).fold(ZERO, |acc, j| acc + rot[j][kcol] * q[j][c]))
                .collect();
            let big = cur.iter().fold(0.0_f64, |m, v| m.max(v.norm()));
            let lead = cur
                .iter()
                .find(|v| v.norm() >= 0.5 * big)
                .copied()
                .unwrap_or(c64::new(1.0, 0.0));
            if lead.re < 0.0 {
                for row in rot.iter_mut() {
                    row[kcol] = -row[kcol];
                }
            }
        }
        unit.rot = rot;
    }

    /// Track the previous channels onto the units of `set`: the real path's
    /// algorithm ([`super::hybrid`] module docs) with unconjugated complex
    /// overlaps and complex-orthogonal Procrustes rotations.
    fn track(
        &mut self,
        set: &LossyHybridModeSet,
        units: &mut [LUnit],
        overlaps: &mut [Option<f64>],
        p_idx: usize,
        omega: f64,
        min_overlap: f64,
    ) -> Result<(), DrivenError> {
        let n_prev = self.prev.len();
        let n_new = set.modes.len();
        let n_units = units.len();
        // Signed (complex) normalized overlaps O_ij = c_ij / (√N_i √N_j).
        let mut o = vec![vec![ZERO; n_new]; n_prev];
        let mut c = vec![vec![ZERO; n_new]; n_prev];
        let w_new: Vec<Vec<c64>> = set
            .modes
            .iter()
            .map(|m| lossy_pairing_vector(&self.ctx.d, m))
            .collect();
        for (i, pm) in self.prev.iter().enumerate() {
            let mut me = vec![ZERO; pm.e_t.len()];
            rmatvec(self.ctx.m1.as_ref(), &pm.e_t, &mut me);
            for (j, (nm, w)) in set.modes.iter().zip(&w_new).enumerate() {
                if nm.degenerate() {
                    continue;
                }
                let v = dot_u(&me, w);
                c[i][j] = v;
                o[i][j] = v / (pm.norm.sqrt() * nm.norm.sqrt());
            }
        }
        let cap: Vec<Vec<f64>> = (0..n_prev)
            .map(|i| {
                units
                    .iter()
                    .map(|u| {
                        u.members
                            .iter()
                            .map(|&j| o[i][j].norm_sqr())
                            .sum::<f64>()
                            .sqrt()
                    })
                    .collect()
            })
            .collect();
        let mut assign = vec![usize::MAX; n_prev];
        let mut load = vec![0usize; n_units];
        let mut cand: Vec<(f64, usize, usize)> = (0..n_prev)
            .flat_map(|i| (0..n_units).map(move |u| (i, u)))
            .map(|(i, u)| (cap[i][u], i, u))
            .collect();
        cand.sort_by(|a, b| b.0.total_cmp(&a.0));
        for &(_, i, u) in &cand {
            if assign[i] == usize::MAX && load[u] < units[u].members.len() {
                assign[i] = u;
                load[u] += 1;
            }
        }
        if assign.contains(&usize::MAX) {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port {p_idx}: fewer modes than channels at ω = {omega}; mode \
                 identity lost — refine the sweep"
            )));
        }
        for grp in lossy_channel_groups(&self.prev, self.prev_scale) {
            let runner_up = |i: usize, own: &[usize]| {
                (0..n_units)
                    .filter(|u| !own.contains(u))
                    .map(|u| cap[i][u])
                    .fold(0.0_f64, f64::max)
            };
            if grp.len() == 1 {
                let i = grp[0];
                let (ov, second) = (cap[i][assign[i]], runner_up(i, &[assign[i]]));
                if ov < min_overlap || second >= min_overlap {
                    return Err(DrivenError::Solve(format!(
                        "hybrid wave port {p_idx}: mode identity lost for lossy channel {i} \
                         between the previous frequency and ω = {omega} (best overlap {ov:.3}, \
                         runner-up {second:.3}, threshold {min_overlap}); modes of the same \
                         family cross or mix here — refine the sweep"
                    )));
                }
                continue;
            }
            let mut own: Vec<usize> = grp.iter().map(|&i| assign[i]).collect();
            own.sort_unstable();
            own.dedup();
            let cols: Vec<usize> = own
                .iter()
                .flat_map(|&u| units[u].members.iter().copied())
                .collect();
            let block: Vec<Vec<c64>> = grp
                .iter()
                .map(|&i| cols.iter().map(|&j| o[i][j]).collect())
                .collect();
            let cos = singular_values(&block);
            let min_cos = cos[cos.len() - 1];
            let second = grp
                .iter()
                .map(|&i| runner_up(i, &own))
                .fold(0.0_f64, f64::max);
            if min_cos < min_overlap || second >= min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: mode identity lost for the degenerate lossy \
                     cluster of channels {grp:?} between the previous frequency and ω = {omega} \
                     (smallest principal cosine {min_cos:.3}, runner-up {second:.3}, threshold \
                     {min_overlap}); refine the sweep"
                )));
            }
            if own.len() > 1 {
                self.cluster_warn.push(PortWarningKind::ClusterSplit {
                    channels: grp.clone(),
                    omega,
                    min_cosine: min_cos,
                });
            }
        }
        for (u, unit) in units.iter_mut().enumerate() {
            let claimed: Vec<usize> = (0..n_prev).filter(|&i| assign[i] == u).collect();
            if claimed.is_empty() {
                continue;
            }
            if unit.members.len() == 1 {
                let (i, j) = (claimed[0], unit.members[0]);
                let sgn = (c[i][j] / set.modes[j].norm).re;
                unit.rot[0][0] = c64::new(if sgn < 0.0 { -1.0 } else { 1.0 }, 0.0);
                unit.claimed = vec![(0, i)];
                overlaps[i] = Some(o[i][j].norm());
                continue;
            }
            let g = unit.members.len();
            let block: Vec<Vec<c64>> = claimed
                .iter()
                .map(|&i| unit.members.iter().map(|&j| o[i][j]).collect())
                .collect();
            let cos = singular_values(&block);
            let min_cos = cos[cos.len() - 1];
            let rot = if min_cos >= min_overlap {
                complex_procrustes(&block, g)
            } else {
                None
            };
            let Some(rot) = rot else {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: lossy channels {claimed:?} map onto one degenerate \
                     cluster at ω = {omega} with smallest principal cosine {min_cos:.3} \
                     (threshold {min_overlap}); mode identity lost — refine the sweep"
                )));
            };
            for (kdir, &i) in claimed.iter().enumerate() {
                let ov = (0..g).fold(ZERO, |acc, j| acc + block[kdir][j] * rot[j][kdir]);
                overlaps[i] = Some(ov.norm());
            }
            unit.rot = rot;
            unit.claimed = claimed.iter().enumerate().map(|(d, &i)| (d, i)).collect();
        }
        Ok(())
    }

    /// Per-channel accuracy at `omega` (and the observed rates at the first
    /// frequency), with the line-impedance estimate of every channel that
    /// has a line (`lines`; the real path's rules, [`super::hybrid_z`]). A
    /// failed refined solve gives `None` (reported as unavailable), never an
    /// error.
    #[allow(clippy::too_many_arguments)]
    fn estimate(
        &mut self,
        port: &HybridWavePort,
        tracked: &[LossyHybridMode],
        lines: &[Option<HybridComplexLineReport>],
        cluster_sizes: &[usize],
        eps: &[c64],
        omega: f64,
        first: bool,
        acc: PortAccuracyOpts,
    ) -> (Vec<Option<ModeAccuracy>>, Vec<ZOutcome>) {
        let unavailable = |why: &str| -> Vec<ZOutcome> {
            lines
                .iter()
                .map(|l| match l {
                    Some(_) => ZOutcome::Unavailable(why.to_string()),
                    None => ZOutcome::NotApplicable,
                })
                .collect()
        };
        let want_z = lines.iter().any(Option::is_some);
        let f = &port.face;
        let refined = self.refined.get_or_insert_with(|| {
            LossyRefinement::new(
                &f.projection.tri_mesh,
                eps,
                &f.interior_edge_mask,
                &f.free_node_mask,
            )
        });
        refined.with_coarse_eps(eps);
        let n_ev_tracked = tracked.iter().filter(|m| !m.is_propagating()).count();
        let opts = HybridPortOpts {
            n_evanescent: n_ev_tracked + 2,
            max_krylov: port.opts.max_krylov.max(4 * 48),
            residual_tol: port.opts.residual_tol,
            ..HybridPortOpts::default()
        };
        let Ok(set_h2) = refined.solve(omega, &opts) else {
            return (
                vec![None; tracked.len()],
                unavailable("the refined (h/2) lossy solve failed"),
            );
        };
        let coarse: Vec<&LossyHybridMode> = tracked.iter().collect();
        let level_h2 =
            || LineLevel::of_face(f).refine(&f.projection.tri_mesh, &f.interior_edge_mask);
        if want_z && self.z_h2.is_none() {
            let r = &refined.refinement;
            self.z_h2 = Some(LineGeom::new(&r.mesh, &r.eps_r, &level_h2()));
        }
        let matched = match_refined_lossy_modes(refined, &coarse, &set_h2);
        if first && acc.observe_rate && self.rates.is_none() {
            let base = lossy_mode_accuracy(refined, &coarse, &set_h2, None);
            let refined2 = self.refined2.get_or_insert_with(|| refined.refine());
            let mut rates = vec![NOMINAL_RATE; tracked.len()];
            let mut z_rates = vec![[None; 3]; tracked.len()];
            if let Ok(set_h4) = refined2.solve(omega, &opts) {
                let mid: Vec<&LossyHybridMode> = matched
                    .iter()
                    .filter_map(|m| m.map(|(j, _)| &set_h2.modes[j]))
                    .collect();
                let fine_m = match_refined_lossy_modes(refined2, &mid, &set_h4);
                let z_h4 = want_z
                    .then(|| {
                        let r = &refined.refinement;
                        let level = level_h2().refine(&r.mesh, &r.interior_edge_mask);
                        let r2 = &refined2.refinement;
                        LineGeom::new(&r2.mesh, &r2.eps_r, &level)
                    })
                    .flatten();
                let mut fine = fine_m.into_iter();
                for (c, b) in base.iter().enumerate() {
                    let Some((j2, _)) = matched[c] else {
                        continue;
                    };
                    // One fine entry per matched coarse mode (`mid` order).
                    let fm = fine.next().flatten();
                    if let (Some(b), Some((j4, _))) = (b, fm) {
                        let m4 = &set_h4.modes[j4];
                        rates[c] = observed_rate(tracked[c].beta, b.beta_refined, m4.beta);
                        if let (Some(l1), Some(Some(g2)), Some(g4)) = (&lines[c], &self.z_h2, &z_h4)
                            && let (Some(l2), Some(l4)) = (
                                g2.line_lossy(&set_h2.modes[j2], omega, &refined.eps_r),
                                g4.line_lossy(m4, omega, &refined2.eps_r),
                            )
                        {
                            z_rates[c] =
                                observed_rates(&line_z_c(l1), &line_z_c(&l2), &line_z_c(&l4));
                        }
                    }
                }
            }
            self.rates = Some(rates);
            self.z_rates = Some(z_rates);
        }
        let beta_acc = lossy_mode_accuracy(refined, &coarse, &set_h2, self.rates.as_deref());
        let z = (0..tracked.len())
            .map(|c| {
                let Some(l1) = &lines[c] else {
                    return ZOutcome::NotApplicable;
                };
                let Some(Some(g2)) = &self.z_h2 else {
                    return ZOutcome::Unavailable(
                        "the refined (h/2) line operators did not assemble".into(),
                    );
                };
                if cluster_sizes[c] > 1 {
                    return ZOutcome::Unavailable(format!(
                        "the channel is one of an exactly degenerate cluster of {} modes, whose \
                         refined counterpart is not unique",
                        cluster_sizes[c]
                    ));
                }
                let Some((j, _)) = matched[c] else {
                    return ZOutcome::Unavailable("no refined (h/2) counterpart matched".into());
                };
                let Some(l2) = g2.line_lossy(&set_h2.modes[j], omega, &refined.eps_r) else {
                    return ZOutcome::Unavailable(
                        "the refined (h/2) counterpart is not propagating".into(),
                    );
                };
                let rates = self.z_rates.as_ref().map_or([None; 3], |r| r[c]);
                match impedance_accuracy(&line_z_c(l1), &line_z_c(&l2), &rates) {
                    Some(a) => ZOutcome::Estimate(a),
                    None => ZOutcome::Unavailable("a refined line impedance is not finite".into()),
                }
            })
            .collect();
        (beta_acc, z)
    }

    /// The port's report and warnings at the end of the sweep.
    pub(super) fn into_report(self, p_idx: usize) -> (HybridPortReport, Vec<PortWarning>) {
        let h = self.ctx.h;
        let mut warnings = Vec::new();
        for kind in self.dropped_warn {
            let PortWarningKind::ComplexPairDropped {
                mode_indices,
                omega,
                unfilled_slots,
            } = &kind
            else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx}: lossy mode {} at ω = {omega} has a degenerate \
                     self-pairing zᵀBz ≈ 0 and cannot be terminated; the evanescent termination \
                     window stops above it ({unfilled_slots} slot(s) unfilled). Refine the port \
                     face below h = {h:.4e}",
                    mode_indices[0]
                ),
                kind,
            });
        }
        for kind in self.acc_warn.into_iter().flatten() {
            let PortWarningKind::AccuracyAboveThreshold {
                channel,
                omega,
                estimate,
                threshold,
                h,
                h_required,
                ez_energy_fraction,
            } = &kind
            else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: estimated β error {:.3} % at \
                     ω = {omega} exceeds {:.3} % (E_z energy fraction {:.2}); refine the port \
                     face to h ≤ {h_required:.4e} (now {h:.4e})",
                    100.0 * estimate,
                    100.0 * threshold,
                    ez_energy_fraction
                ),
                kind,
            });
        }
        for kind in self.alpha_warn.into_iter().flatten() {
            let PortWarningKind::AttenuationAccuracyAboveThreshold {
                channel,
                omega,
                estimate,
                threshold,
                h,
                h_required,
            } = &kind
            else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: estimated attenuation (α) error \
                     {:.3} % at ω = {omega} exceeds {:.3} %; refine the port face to h ≤ \
                     {h_required:.4e} (now {h:.4e})",
                    100.0 * estimate,
                    100.0 * threshold,
                ),
                kind,
            });
        }
        for kind in self.acc_unavail.into_iter().flatten() {
            let PortWarningKind::AccuracyUnavailable { channel, omega } = &kind else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: no refined (h/2) counterpart \
                     matched (or the refined lossy solve failed) at ω = {omega}; accuracy \
                     estimate unavailable"
                ),
                kind,
            });
        }
        warnings.extend(self.z_warn.into_warnings(p_idx));
        if let Some((omega, n_omegas, verified)) = self.mult_warn {
            warnings.push(multiplicity_warning(p_idx, omega, n_omegas, verified));
        }
        warnings.extend(
            self.cluster_warn
                .into_iter()
                .map(|kind| cluster_warning(p_idx, kind)),
        );
        let report = HybridPortReport {
            port: p_idx,
            n_conductors: self.ctx.conductors.len(),
            mesh_size: h,
            points: self
                .points
                .into_iter()
                .map(|p| p.expect("every frequency visited"))
                .collect(),
            observed_rates: self.rates,
        };
        (report, warnings)
    }
}

/// [`HybridComplexLineReport`] of a propagating lossy mode on face `ε` at
/// `k0` (`None` without conductors or for `Re β² ≤ 0`).
fn lossy_line(
    ctx: &FaceCtx,
    port: &HybridWavePort,
    m: &LossyHybridMode,
    k0: f64,
    eps: &[c64],
) -> Option<HybridComplexLineReport> {
    line_complex(
        &ctx.blocks,
        &ctx.d,
        &port.face.projection.tri_mesh,
        &ctx.conductors,
        &ctx.paths,
        m,
        k0,
        eps,
    )
}

/// Exactly degenerate clusters of lossy `modes` (the solver's order):
/// maximal runs with `|Δβ²| ≤ DEGENERATE_REL_TOL · scale` among modes with a
/// regular self-pairing (the lossy solver's own cluster rule). A simple
/// eigenvalue, and every degenerate-pairing mode, is a unit of one.
fn lossy_units(modes: &[LossyHybridMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (j, m) in modes.iter().enumerate() {
        if let Some(u) = out.last_mut() {
            let f = &modes[u[0]];
            if !m.degenerate() && !f.degenerate() && (m.beta_sq - f.beta_sq).norm() <= tol {
                u.push(j);
                continue;
            }
        }
        out.push(vec![j]);
    }
    out
}

/// The same partition for an unordered channel list.
fn lossy_channel_groups(chs: &[LossyHybridMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (i, m) in chs.iter().enumerate() {
        match out
            .iter_mut()
            .find(|g| (m.beta_sq - chs[g[0]].beta_sq).norm() <= tol)
        {
            Some(g) => g.push(i),
            None => out.push(vec![i]),
        }
    }
    out
}

/// `Σ_j c_j m_j` over an exactly degenerate lossy cluster with complex
/// orthonormal coefficients (`Σ c_j² = 1`, so `zᵀBz` stays the cluster's
/// `β²`); residuals and floors are the members' maximum, the transverse
/// fraction the `|c|²`-weighted mean, the conditioning the minimum.
fn combine_lossy(members: &[&LossyHybridMode], c: &[c64]) -> LossyHybridMode {
    let f = members[0];
    let mut out = f.clone();
    out.e_t.iter_mut().for_each(|v| *v = ZERO);
    out.e_z.iter_mut().for_each(|v| *v = ZERO);
    out.residual = 0.0;
    out.residual_floor = 0.0;
    out.transverse_fraction = 0.0;
    let wsum: f64 = c
        .iter()
        .map(|x| x.norm_sqr())
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    for (m, &cj) in members.iter().zip(c) {
        for (o, v) in out.e_t.iter_mut().zip(&m.e_t) {
            *o += cj * v;
        }
        for (o, v) in out.e_z.iter_mut().zip(&m.e_z) {
            *o += cj * v;
        }
        out.residual = out.residual.max(m.residual);
        out.residual_floor = out.residual_floor.max(m.residual_floor);
        out.transverse_fraction += cj.norm_sqr() / wsum * m.transverse_fraction;
        out.conditioning = out.conditioning.min(m.conditioning);
    }
    out
}

/// Singular values (descending) of a small complex `q × g` block, from the
/// eigenvalues of the Hermitian `OOᴴ` through its real symmetric embedding
/// `[[Re, −Im], [Im, Re]]` (each eigenvalue appears twice).
fn singular_values(o: &[Vec<c64>]) -> Vec<f64> {
    let q = o.len();
    let mut h = vec![vec![ZERO; q]; q];
    for a in 0..q {
        for b in 0..q {
            h[a][b] = o[a]
                .iter()
                .zip(&o[b])
                .fold(ZERO, |acc, (x, y)| acc + x * y.conj());
        }
    }
    let mut e = vec![vec![0.0; 2 * q]; 2 * q];
    for a in 0..q {
        for b in 0..q {
            e[a][b] = h[a][b].re;
            e[a + q][b + q] = h[a][b].re;
            e[a][b + q] = -h[a][b].im;
            e[a + q][b] = h[a][b].im;
        }
    }
    let (lam, _) = sym_eig(&e);
    lam.iter().step_by(2).map(|l| l.max(0.0).sqrt()).collect()
}

/// Row-major flat copy of a small square complex matrix.
fn flat(a: &[Vec<c64>]) -> Vec<c64> {
    a.iter().flatten().copied().collect()
}

fn matmul(a: &[c64], b: &[c64], n: usize) -> Vec<c64> {
    let mut out = vec![ZERO; n * n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i * n + k];
            for j in 0..n {
                out[i * n + j] += aik * b[k * n + j];
            }
        }
    }
    out
}

/// `A^{-1/2}` of a small complex symmetric matrix near the identity
/// (Denman–Beavers; the principal root, itself complex symmetric). `None`
/// if it does not converge or a step is singular.
fn csym_inv_sqrt(a: &[Vec<c64>]) -> Option<Vec<c64>> {
    let n = a.len();
    let mut y = flat(a);
    let mut z: Vec<c64> = (0..n * n)
        .map(|i| {
            if i % (n + 1) == 0 {
                c64::new(1.0, 0.0)
            } else {
                ZERO
            }
        })
        .collect();
    for _ in 0..60 {
        let yi = invert_complex_dense(&y, n)?;
        let zi = invert_complex_dense(&z, n)?;
        let y2: Vec<c64> = y.iter().zip(&zi).map(|(p, q)| (p + q) * 0.5).collect();
        let z2: Vec<c64> = z.iter().zip(&yi).map(|(p, q)| (p + q) * 0.5).collect();
        let dz: f64 = z2.iter().zip(&z).map(|(p, q)| (p - q).norm_sqr()).sum();
        y = y2;
        z = z2;
        if dz <= 1e-30 {
            // Check Z A Z = I.
            let check = matmul(&matmul(&z, &flat(a), n), &z, n);
            let err: f64 = check
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    (v - if i % (n + 1) == 0 {
                        c64::new(1.0, 0.0)
                    } else {
                        ZERO
                    })
                    .norm()
                })
                .fold(0.0, f64::max);
            return (err <= 1e-9).then_some(z);
        }
    }
    None
}

/// Complex-orthogonal Procrustes of a degenerate lossy cluster onto `q`
/// previous channels (`o`: `q × g`): the `g × g` complex orthogonal `R`
/// (`RᵀR = I`) whose first `q` columns are `oᵀ(ooᵀ)^{-1/2}` and whose others
/// complete the basis (unconjugated Gram–Schmidt). `None` if the inverse
/// square root or the completion breaks down.
fn complex_procrustes(o: &[Vec<c64>], g: usize) -> Option<Vec<Vec<c64>>> {
    let q = o.len();
    let oot: Vec<Vec<c64>> = (0..q)
        .map(|a| {
            (0..q)
                .map(|b| (0..g).fold(ZERO, |acc, j| acc + o[a][j] * o[b][j]))
                .collect()
        })
        .collect();
    let zi = csym_inv_sqrt(&oot)?;
    let mut cols: Vec<Vec<c64>> = (0..q)
        .map(|k| {
            (0..g)
                .map(|j| (0..q).fold(ZERO, |acc, i| acc + o[i][j] * zi[i * q + k]))
                .collect()
        })
        .collect();
    while cols.len() < g {
        // The unit vector least represented so far, orthogonalized.
        let best = (0..g)
            .map(|e| {
                let mut v: Vec<c64> = (0..g)
                    .map(|j| if j == e { c64::new(1.0, 0.0) } else { ZERO })
                    .collect();
                for _ in 0..2 {
                    for cvec in &cols {
                        let d = cvec.iter().zip(&v).fold(ZERO, |acc, (a, b)| acc + a * b);
                        for (vi, ci) in v.iter_mut().zip(cvec) {
                            *vi -= d * ci;
                        }
                    }
                }
                let nn = v.iter().fold(ZERO, |acc, x| acc + x * x);
                (nn.norm(), nn, v)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        if best.0 <= 1e-8 {
            return None;
        }
        let inv = best.1.sqrt().inv();
        cols.push(best.2.into_iter().map(|x| x * inv).collect());
    }
    Some(
        (0..g)
            .map(|j| cols.iter().map(|c| c[j]).collect())
            .collect(),
    )
}

/// How a lossy face unit is used at one frequency (the real path's
/// `UnitUse` with complex rotations).
struct LUnit {
    members: Vec<usize>,
    rot: Vec<Vec<c64>>,
    claimed: Vec<(usize, usize)>,
}

impl LUnit {
    fn identity(members: Vec<usize>) -> Self {
        let g = members.len();
        Self {
            rot: (0..g)
                .map(|i| {
                    (0..g)
                        .map(|j| if i == j { c64::new(1.0, 0.0) } else { ZERO })
                        .collect()
                })
                .collect(),
            members,
            claimed: Vec::new(),
        }
    }

    fn direction(&self, modes: &[LossyHybridMode], k: usize) -> LossyHybridMode {
        if self.members.len() == 1 {
            let mut m = modes[self.members[0]].clone();
            if self.rot[0][0].re < 0.0 {
                m.e_t.iter_mut().for_each(|v| *v = -*v);
                m.e_z.iter_mut().for_each(|v| *v = -*v);
            }
            return m;
        }
        let mem: Vec<&LossyHybridMode> = self.members.iter().map(|&j| &modes[j]).collect();
        let c: Vec<c64> = (0..self.members.len()).map(|j| self.rot[j][k]).collect();
        combine_lossy(&mem, &c)
    }
}

/// [`HybridWavePort::modal_fluxes`] for a lossy face (its own complex `ε`).
pub(super) fn lossy_modal_fluxes(
    port: &HybridWavePort,
    mesh: &TetMesh,
    omega: f64,
) -> Result<Vec<HybridModalFlux>, DrivenError> {
    let edges = mesh.edges();
    let ctx = FaceCtx::new(mesh, &edges, port, 0)?;
    let eps = face_eps(port, None, 0, omega)?;
    let (set, _) = solve(port, &eps, omega, 0)?;
    let k = port.n_modes();
    if set.modes.len() < k {
        return Err(DrivenError::Solve(format!(
            "hybrid wave port: {} lossy modes at ω = {omega}, {k} requested",
            set.modes.len()
        )));
    }
    Ok(set
        .modes
        .iter()
        .take(k)
        .map(|m| {
            let (flux_local, e_local) = channel(&ctx, m, port.opts.transverse_only_flux);
            let mut e_t = vec![ZERO; edges.len()];
            let mut flux = vec![ZERO; edges.len()];
            for (l, &g) in ctx.lift.iter().enumerate() {
                e_t[g] = e_local[l];
            }
            for (g, v) in flux_local {
                flux[g] = v;
            }
            HybridModalFlux {
                beta: m.beta,
                e_t,
                flux,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::microstrip::{ShieldedStripFace, StripMeshOpts};
    use crate::driven::ports::{HybridPortFace, HybridWavePortOpts, strip_line_section};

    fn rel_ratio(i: &[c64], want: f64) -> f64 {
        (i[1] / i[0] - want).norm()
    }

    /// The lossy cluster machinery on a homogeneous lossy two-strip stripline
    /// (`ε = 2.2 − 0.022j`): the canonical basis is odd / even (`I₂/I₁ = ∓1`
    /// to 1e-8); after rotating the stored channels 45° inside the cluster the
    /// per-vector overlaps are ambiguous (both above the threshold), while the
    /// complex-orthogonal Procrustes tracker lands back on the rotated basis
    /// (overlaps ≥ 1 − 1e-9, same current ratios); the helpers are exact.
    #[test]
    fn lossy_cluster_is_canonical_and_survives_an_arbitrary_basis() {
        // Helpers: csym inverse square root and complex Procrustes.
        let a = vec![
            vec![c64::new(1.1, 0.05), c64::new(0.2, -0.01)],
            vec![c64::new(0.2, -0.01), c64::new(0.9, 0.02)],
        ];
        let z = csym_inv_sqrt(&a).expect("inverse sqrt");
        let check = matmul(&matmul(&z, &flat(&a), 2), &z, 2);
        for (i, v) in check.iter().enumerate() {
            let want = if i % 3 == 0 { 1.0 } else { 0.0 };
            assert!((v - c64::new(want, 0.0)).norm() < 1e-12, "{check:?}");
        }
        let r = complex_procrustes(&[vec![c64::new(0.6, 0.01), c64::new(0.8, -0.02)]], 2).unwrap();
        for p in 0..2 {
            for q in 0..2 {
                let v = (0..2).fold(ZERO, |acc, j| acc + r[j][p] * r[j][q]);
                let want = if p == q { 1.0 } else { 0.0 };
                assert!((v - c64::new(want, 0.0)).norm() < 1e-12);
            }
        }

        let eps = c64::new(2.2, -0.022);
        let face = ShieldedStripFace {
            box_width: 8.0,
            box_height: 4.0,
            h: 2.0,
            strips: vec![[-1.75, -0.25], [0.25, 1.75]],
            thickness: 0.0,
            eps_below: eps.re,
            eps_above: eps.re,
        }
        .build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let mesh = &sec.extruded.mesh;
        let edges = mesh.edges();
        let eps_c = vec![eps; mesh.n_tets()];
        let f1 = HybridPortFace::from_volume_lossy(mesh, &sec.extruded.port1_faces, &eps_c)
            .unwrap()
            .with_interior_pec(&edges, &sec.pec_interior_mask)
            .unwrap();
        let port =
            HybridWavePort::new(f1, vec![c64::new(1.0, 0.0); 2]).with_opts(HybridWavePortOpts {
                accuracy: None,
                ..Default::default()
            });
        let ctx = FaceCtx::new(mesh, &edges, &port, 0).unwrap();
        let mut st = LossyState::new(ctx, &port, 2);
        st.channels_at(&port, 0, 0.1, 0, true, None).unwrap();
        assert!(st.cluster_warn.is_empty(), "{:?}", st.cluster_warn);
        let cur: Vec<Vec<c64>> = st
            .prev
            .iter()
            .map(|m| st.ctx.currents_c(&m.e_t, &m.e_z, 0.1))
            .collect();
        println!(
            "canonical lossy cluster: I₂/I₁ = {}, {}",
            cur[0][1] / cur[0][0],
            cur[1][1] / cur[1][0]
        );
        assert!(rel_ratio(&cur[0], -1.0) <= 1e-8, "odd first");
        assert!(rel_ratio(&cur[1], 1.0) <= 1e-8, "even second");

        // Rotate the stored channels by 45° inside the cluster.
        let (a0, b0) = (st.prev[0].clone(), st.prev[1].clone());
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let rot = [
            combine_lossy(&[&a0, &b0], &[c64::new(c, 0.0), c64::new(c, 0.0)]),
            combine_lossy(&[&a0, &b0], &[c64::new(-c, 0.0), c64::new(c, 0.0)]),
        ];
        st.prev = rot.to_vec();
        let eps_face = vec![eps; port.face.eps_r.len()];
        let (set, _) = solve(&port, &eps_face, 0.12, 2).unwrap();
        let mut me = vec![ZERO; st.prev[0].e_t.len()];
        rmatvec(st.ctx.m1.as_ref(), &st.prev[0].e_t, &mut me);
        let ov: Vec<f64> = set
            .modes
            .iter()
            .take(2)
            .map(|m| {
                let w = lossy_pairing_vector(&st.ctx.d, m);
                dot_u(&me, &w).norm() / (st.prev[0].norm.norm() * m.norm.norm()).sqrt()
            })
            .collect();
        println!("per-vector overlaps of rotated lossy channel 0: {ov:?}");
        assert!(
            ov.iter()
                .all(|&o| o >= crate::driven::ports::DEFAULT_MIN_TRACK_OVERLAP)
        );
        st.channels_at(&port, 0, 0.12, 1, false, None).unwrap();
        let pt = st.points[1].as_ref().unwrap();
        for ch in &pt.channels {
            assert!(ch.track_overlap.unwrap() >= 1.0 - 1e-9);
            assert_eq!(ch.cluster_size, 2);
        }
        for (m, want) in st.prev.iter().zip(&rot) {
            let (i, iw) = (
                st.ctx.currents_c(&m.e_t, &m.e_z, 0.12),
                st.ctx.currents_c(&want.e_t, &want.e_z, 0.1),
            );
            assert!(
                (i[1] / i[0] - iw[1] / iw[0]).norm() <= 1e-8,
                "{i:?} vs {iw:?}"
            );
        }
        assert!(st.cluster_warn.is_empty());
    }

    /// `ClusterSplit` on the lossy path: the inhomogeneous lossy coupled
    /// microstrip (`ε = 4.4(1 − 0.01j)` below, vacuum above) has distinct
    /// even/odd β². Storing the 45° combinations of the pair with one shared
    /// β² as the previous channels emulates a tracked cluster that splits at
    /// the next frequency: the subspace is captured over two units, so the
    /// tracker assigns inside it and records the split.
    #[test]
    fn lossy_cluster_split_fires_when_a_tracked_cluster_becomes_two_simple_modes() {
        let face = ShieldedStripFace {
            box_width: 8.0,
            box_height: 4.0,
            h: 2.0,
            strips: vec![[-1.75, -0.25], [0.25, 1.75]],
            thickness: 0.0,
            eps_below: 4.4,
            eps_above: 1.0,
        }
        .build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let mesh = &sec.extruded.mesh;
        let edges = mesh.edges();
        let eps_c: Vec<c64> = sec
            .eps_tet
            .iter()
            .map(|&e| {
                if e > 1.0 {
                    c64::new(e, -0.01 * e)
                } else {
                    c64::new(e, 0.0)
                }
            })
            .collect();
        let f1 = HybridPortFace::from_volume_lossy(mesh, &sec.extruded.port1_faces, &eps_c)
            .unwrap()
            .with_interior_pec(&edges, &sec.pec_interior_mask)
            .unwrap();
        let port =
            HybridWavePort::new(f1, vec![c64::new(1.0, 0.0); 2]).with_opts(HybridWavePortOpts {
                accuracy: None,
                ..Default::default()
            });
        let ctx = FaceCtx::new(mesh, &edges, &port, 0).unwrap();
        let mut st = LossyState::new(ctx, &port, 2);
        st.channels_at(&port, 0, 0.1, 0, true, None).unwrap();
        assert_eq!(st.prev.len(), 2);
        assert!(st.cluster_warn.is_empty(), "{:?}", st.cluster_warn);
        assert!(
            (st.prev[0].beta_sq - st.prev[1].beta_sq).norm() > DEGENERATE_REL_TOL * st.prev_scale
        );
        let (a0, b0) = (st.prev[0].clone(), st.prev[1].clone());
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let mut rot = [
            combine_lossy(&[&a0, &b0], &[c64::new(c, 0.0), c64::new(c, 0.0)]),
            combine_lossy(&[&a0, &b0], &[c64::new(-c, 0.0), c64::new(c, 0.0)]),
        ];
        for m in &mut rot {
            m.beta_sq = a0.beta_sq;
        }
        st.prev = rot.to_vec();
        assert_eq!(
            lossy_channel_groups(&st.prev, st.prev_scale),
            vec![vec![0, 1]]
        );
        st.channels_at(&port, 0, 0.12, 1, false, None).unwrap();
        println!("lossy cluster_warn: {:?}", st.cluster_warn);
        assert_eq!(st.cluster_warn.len(), 1);
        match &st.cluster_warn[0] {
            PortWarningKind::ClusterSplit {
                channels,
                omega,
                min_cosine,
            } => {
                assert_eq!(channels, &vec![0, 1]);
                assert_eq!(*omega, 0.12);
                assert!(
                    *min_cosine >= crate::driven::ports::DEFAULT_MIN_TRACK_OVERLAP
                        && *min_cosine < 1.0 - 1e-6,
                    "min cosine {min_cosine}"
                );
            }
            w => panic!("expected ClusterSplit, got {w:?}"),
        }
        let pt = st.points[1].as_ref().unwrap();
        assert_eq!(pt.channels.len(), 2);
        for ch in &pt.channels {
            let ovl = ch.track_overlap.unwrap();
            println!("lossy split-channel overlap {ovl:.4}");
            assert!(
                (crate::driven::ports::DEFAULT_MIN_TRACK_OVERLAP..1.0 - 1e-6).contains(&ovl),
                "overlap {ovl}"
            );
        }
    }
}
