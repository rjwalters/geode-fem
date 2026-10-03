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
//!   errors as Phase 2.
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
//!   (the #815 review's first follow-up note, applied to this path).
//!
//! Touchstone (#775) renormalizes with a characteristic impedance; a lossy
//! hybrid channel's is complex and frequency dependent, which the Γ-form
//! renormalization to a real `reference_ohm` accepts. Which `Z_c`
//! definition hybrid ports use is Phase 5's decision (#807).

use faer::c64;

use super::hybrid::{
    ChanAt, FaceCtx, HybridChannelReport, HybridModalFlux, HybridPortPointReport, HybridPortReport,
    HybridWavePort, PortAccuracyOpts, PortWarning, PortWarningKind, check_lossy_eps,
};
use crate::analytic::lossy_port_modes::{
    LossyHybridMode, LossyHybridModeSet, LossyRefinement, dot_u, lossy_alpha_estimate,
    lossy_mode_accuracy, lossy_pairing_vector, match_refined_lossy_modes, rmatvec,
    solve_lossy_hybrid_port_modes,
};
use crate::analytic::port_mode_accuracy::{ModeAccuracy, NOMINAL_RATE, observed_rate};
use crate::analytic::port_modes::{HybridPortError, HybridPortOpts};
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
    points: Vec<Option<HybridPortPointReport>>,
    dropped_warn: Vec<PortWarningKind>,
    acc_warn: Vec<Option<PortWarningKind>>,
    acc_unavail: Vec<Option<PortWarningKind>>,
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
/// Phase 2).
fn solve(
    port: &HybridWavePort,
    eps: &[c64],
    omega: f64,
    extra: usize,
) -> Result<LossyHybridModeSet, DrivenError> {
    let base = port.n_modes() + port.opts.n_termination_evanescent;
    let mut opts = HybridPortOpts {
        n_evanescent: base + extra,
        max_krylov: port.opts.max_krylov,
        residual_tol: port.opts.residual_tol,
        ..HybridPortOpts::default()
    };
    let f = &port.face;
    let run = |o: &HybridPortOpts| {
        solve_lossy_hybrid_port_modes(
            &f.projection.tri_mesh,
            eps,
            &f.projection.interior_edge_mask,
            &f.free_node_mask,
            omega,
            o,
        )
    };
    match run(&opts) {
        Ok(s) => Ok(s),
        Err(HybridPortError::Shortfall { n_propagating, .. }) if extra > 0 => {
            opts.n_evanescent = base.saturating_sub(n_propagating);
            run(&opts)
        }
        Err(e) => Err(e),
    }
    .map_err(|e| {
        DrivenError::Solve(format!(
            "hybrid wave port: lossy port-mode solve at ω = {omega} failed: {e}"
        ))
    })
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
            points: vec![None; n_omegas],
            dropped_warn: Vec::new(),
            acc_warn: vec![None; port.n_modes()],
            acc_unavail: vec![None; port.n_modes()],
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
        let set = solve(port, &eps, omega, 2)?;
        let n_prop = set.n_propagating;

        let assigned: Vec<(usize, f64, Option<f64>)> = if first || self.prev.is_empty() {
            if set.modes.len() < k {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: only {} modes at ω = {omega}, {k} channels \
                     requested",
                    set.modes.len()
                )));
            }
            (0..k).map(|i| (i, 1.0, None)).collect()
        } else {
            self.track(&set, p_idx, omega, port.opts.min_track_overlap)?
        };
        for (pos, &(i, _, _)) in assigned.iter().enumerate() {
            if set.modes[i].degenerate() {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "hybrid wave port: reported channel {pos} at ω = {omega} has a degenerate \
                         self-pairing zᵀBz ≈ 0 (β² = {}) and cannot be normalized; report fewer \
                         channels or refine the port face (h = {:.4e})",
                        set.modes[i].beta_sq, self.ctx.h
                    ),
                });
            }
        }

        // Completeness: every propagating (Re β² > 0) mode is a reported channel.
        for (i, m) in set.modes.iter().enumerate().take(n_prop) {
            if !assigned.iter().any(|&(a, _, _)| a == i) {
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

        let tracked: Vec<LossyHybridMode> = assigned
            .iter()
            .map(|&(i, s, _)| {
                let mut m = set.modes[i].clone();
                if s < 0.0 {
                    m.e_t.iter_mut().for_each(|v| *v = -*v);
                    m.e_z.iter_mut().for_each(|v| *v = -*v);
                }
                m
            })
            .collect();
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

        // Termination window: the next modes, one slot each.
        let want = port.opts.n_termination_evanescent;
        let mut termination = Vec::new();
        for (i, m) in set.modes.iter().enumerate() {
            if termination.len() >= want {
                break;
            }
            if assigned.iter().any(|&(a, _, _)| a == i) {
                continue;
            }
            if m.degenerate() {
                self.dropped_warn.push(PortWarningKind::ComplexPairDropped {
                    mode_indices: [i, i],
                    omega,
                    unfilled_slots: want - termination.len(),
                });
                break;
            }
            let (flux, _) = channel(&self.ctx, m, port.opts.transverse_only_flux);
            termination.push(ChanAt {
                beta: m.beta,
                y: m.beta,
                a_inc: None,
                flux,
            });
        }
        let term_real = termination.len();

        // Accuracy estimate.
        let mut accuracy: Vec<Option<ModeAccuracy>> = vec![None; k];
        if let Some(acc) = port.opts.accuracy {
            accuracy = self.estimate(port, &tracked, &eps, omega, first, acc);
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
            }
        }

        self.points[input_index] = Some(HybridPortPointReport {
            omega,
            channels: tracked
                .iter()
                .zip(&assigned)
                .zip(accuracy)
                .map(|((m, &(_, _, ov)), a)| HybridChannelReport {
                    beta: m.beta,
                    beta_sq: m.beta_sq.re,
                    beta_sq_im: m.beta_sq.im,
                    ez_energy_fraction: 1.0 - m.transverse_fraction,
                    track_overlap: ov,
                    alpha_accuracy: a.as_ref().and_then(lossy_alpha_estimate),
                    accuracy: a,
                    residual: m.residual,
                })
                .collect(),
            n_propagating: n_prop,
            termination_real: term_real,
            termination_pairs: 0,
        });
        self.prev = tracked;
        Ok((reported, termination))
    }

    /// Greedy one-to-one unconjugated B-pairing assignment of the previous
    /// channels to the modes of `set` (module docs).
    fn track(
        &self,
        set: &LossyHybridModeSet,
        p_idx: usize,
        omega: f64,
        min_overlap: f64,
    ) -> Result<Vec<(usize, f64, Option<f64>)>, DrivenError> {
        let n_prev = self.prev.len();
        let n_new = set.modes.len();
        let mut o = vec![vec![0.0_f64; n_new]; n_prev];
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
                o[i][j] = v.norm() / (pm.norm.norm() * nm.norm.norm()).sqrt();
            }
        }
        let mut row_done = vec![false; n_prev];
        let mut col_used = vec![false; n_new];
        let mut out = vec![(usize::MAX, 1.0, None); n_prev];
        for _ in 0..n_prev {
            let mut best = (usize::MAX, usize::MAX, -1.0_f64);
            for i in (0..n_prev).filter(|&i| !row_done[i]) {
                for j in (0..n_new).filter(|&j| !col_used[j]) {
                    if o[i][j] > best.2 {
                        best = (i, j, o[i][j]);
                    }
                }
            }
            let (i, j, ov) = best;
            if i == usize::MAX {
                break;
            }
            row_done[i] = true;
            col_used[j] = true;
            let second = (0..n_new)
                .filter(|&jj| jj != j)
                .map(|jj| o[i][jj])
                .fold(0.0_f64, f64::max);
            if ov < min_overlap || second >= min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: mode identity lost for lossy channel {i} between \
                     the previous frequency and ω = {omega} (best overlap {ov:.3}, runner-up \
                     {second:.3}, threshold {min_overlap}); modes of the same family cross or \
                     mix here — refine the sweep"
                )));
            }
            let sgn = (c[i][j] / set.modes[j].norm).re;
            out[i] = (j, if sgn < 0.0 { -1.0 } else { 1.0 }, Some(ov));
        }
        if out.iter().any(|&(j, _, _)| j == usize::MAX) {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port {p_idx}: fewer modes than channels at ω = {omega}; mode \
                 identity lost — refine the sweep"
            )));
        }
        Ok(out)
    }

    /// Per-channel accuracy at `omega` (and the observed rates at the first
    /// frequency). A failed refined solve gives `None` (reported as
    /// unavailable), never an error.
    fn estimate(
        &mut self,
        port: &HybridWavePort,
        tracked: &[LossyHybridMode],
        eps: &[c64],
        omega: f64,
        first: bool,
        acc: PortAccuracyOpts,
    ) -> Vec<Option<ModeAccuracy>> {
        let f = &port.face;
        let refined = self.refined.get_or_insert_with(|| {
            LossyRefinement::new(
                &f.projection.tri_mesh,
                eps,
                &f.projection.interior_edge_mask,
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
            return vec![None; tracked.len()];
        };
        let coarse: Vec<&LossyHybridMode> = tracked.iter().collect();
        if first && acc.observe_rate && self.rates.is_none() {
            let base = lossy_mode_accuracy(refined, &coarse, &set_h2, None);
            let matched = match_refined_lossy_modes(refined, &coarse, &set_h2);
            let refined2 = self.refined2.get_or_insert_with(|| refined.refine());
            let mut rates = vec![NOMINAL_RATE; tracked.len()];
            if let Ok(set_h4) = refined2.solve(omega, &opts) {
                let mid: Vec<&LossyHybridMode> = matched
                    .iter()
                    .filter_map(|m| m.map(|(j, _)| &set_h2.modes[j]))
                    .collect();
                let mut fine = lossy_mode_accuracy(refined2, &mid, &set_h4, None).into_iter();
                for (c, b) in base.iter().enumerate() {
                    if matched[c].is_none() {
                        continue;
                    }
                    // One fine entry per matched coarse mode (`mid` order).
                    let fm = fine.next().flatten();
                    if let (Some(b), Some(fm)) = (b, fm) {
                        rates[c] = observed_rate(tracked[c].beta, b.beta_refined, fm.beta_refined);
                    }
                }
            }
            self.rates = Some(rates);
        }
        lossy_mode_accuracy(refined, &coarse, &set_h2, self.rates.as_deref())
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
        let report = HybridPortReport {
            port: p_idx,
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

/// [`HybridWavePort::modal_fluxes`] for a lossy face (its own complex `ε`).
pub(super) fn lossy_modal_fluxes(
    port: &HybridWavePort,
    mesh: &TetMesh,
    omega: f64,
) -> Result<Vec<HybridModalFlux>, DrivenError> {
    let edges = mesh.edges();
    let ctx = FaceCtx::new(mesh, &edges, port, 0)?;
    let eps = face_eps(port, None, 0, omega)?;
    let set = solve(port, &eps, omega, 0)?;
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
