//! Mixed **lumped + wave** port S-parameter sweep (Epic #756 Phase 8c,
//! issue #759).
//!
//! Lumped ports ([`LumpedPort`]) and wave (modal) ports ([`WavePort`])
//! terminate the same complex-symmetric curl-curl operator `A(ω)`: a
//! lumped port adds the resistive sheet load `(jω/Z_s) S_p` and a wave
//! port adds the rank-`K_p` modal admittance `Σ_m jβ_m f_m f_mᵀ`. This
//! module composes the two in one operator:
//!
//! * the lumped loads go into the base operator through
//!   [`DrivenOperator::assemble`] (exactly as the lumped-port sweep
//!   does), so `A_base(ω) = K − ω²M + jωC + Σ_k (jω/Z_s,k) S_k`;
//! * the modal terms are folded in per frequency by the same rank-`N_w`
//!   Sherman-Morrison-Woodbury update as
//!   [`super::solve_wave_port_sweep_with_mode`].
//!
//! Each of the `N_l + N_w` excitations is one SMW-corrected solve: a
//! lumped port `j` is driven with its matched-source Thévenin RHS
//! `(2jω/Z_s)(V_inc/l) g_j` (the RHS `solve_excited(j)` of the lumped
//! sweep uses), a wave channel with `2jβ a_inc f_j`. Every other port is a
//! passive matched termination.
//!
//! # Power-wave S normalization
//!
//! A mixed network has no impedance matrix (a modal channel defines no
//! port voltage), so the lumped sweep's `Z → S` route is not available.
//! The S-matrix is formed directly from power waves. In solver natural
//! units (`η₀ = μ₀ = 1`, `ω = k₀`, `R` in units of η₀) the incident and
//! outgoing amplitudes, normalized so `|ã|²` is the power up to one shared
//! constant, are
//!
//! ```text
//! lumped port k (Thévenin 2·V_inc behind R_k):
//!     ã_k = V_inc,k / √R_k          b̃_k = (V_k − V_inc,k δ) / √R_k
//! wave channel q (TE modal power |a|²β/(2ωμ), S_p-orthonormal modes):
//!     ã_q = a_inc,q · √(β_q/ω)       b̃_q = (a_q − a_inc,q δ) · √(β_q/ω)
//! ```
//!
//! and `S_ij = b̃_i / ã_j`. Block by block:
//!
//! ```text
//! lumped ← lumped:  S_kj = (V_k − V_inc,j δ_kj) / V_inc,j · √(R_j / R_k)
//! wave   ← wave:    S_qp = (a_q − a_inc,p δ_qp) / a_inc,p · √β_q / √β_p
//! wave   ← lumped:  S_qj = a_q · (√β_q / √ω) · √R_j / V_inc,j
//! lumped ← wave:    S_kp = (V_k / √R_k) / (a_inc,p · √β_p / √ω)
//! ```
//!
//! The lumped block is the lumped sweep's real-reference power-wave
//! `F(Z − Z₀)(Z + Z₀)⁻¹F⁻¹` with `Z₀ = diag(R)` (the port's own
//! termination is its reference). The wave block is the wave sweep's
//! power-normalized modal S. The cross weight `√(β/ω)` is fixed by the
//! symmetry `A(ω)ᵀ = A(ω)`: for a lumped RHS `b_l` and a wave RHS `b_w`,
//! `b_wᵀ E⁽ˡ⁾ = b_lᵀ E⁽ʷ⁾` reads `2jβ_q a_inc,q a_q⁽ˡ⁾ = 2jω V_inc,j
//! V_j⁽ʷ⁾ / R_j` (using `g_jᵀ E = w V`, `Z_s = R w / l`), which is exactly
//! `S_qj = S_jq` under the weights above. `√β` is the principal-branch
//! root, as on the wave path, so evanescent channels (`β = −j|β|`) keep
//! `Sᵀ = S`.
//!
//! **Caveat.** The modal admittance `jβ` is the TE form (`1/Z_TE =
//! β/ωμ`). The port-face eigensolver filters TEM modes, and TM-mode wave
//! ports are already approximate on the pure-wave path; mixing in lumped
//! ports does not change that.
//!
//! # Channel order
//!
//! Lumped ports first (input order), then each wave port's modes
//! (port-major, mode-minor): flat index `N_l + Σ_{q<p} K_q + m`.
//!
//! The pure-lumped ([`crate::driven::extraction::s_parameter_point`]) and
//! pure-wave ([`super::solve_wave_port_sweep_with_mode`]) paths are
//! separate functions and are not routed through this one.

use faer::c64;

use super::lumped::LumpedPort;
use super::wave::{WavePort, assemble_modal_flux, invert_complex_dense};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
    SurfaceImpedanceBc, validate_driven_surfaces,
};
use crate::mesh::TetMesh;

/// One frequency point of a mixed lumped + wave port sweep
/// ([`solve_mixed_port_sweep_with_mode`]).
#[derive(Debug, Clone)]
pub struct MixedPortSweepPoint {
    /// Frequency `ω ≡ k₀` (natural units).
    pub omega: f64,
    /// Worst relative residual `‖A_total x − b‖ / ‖b‖` over the
    /// `n_ports` excitation solves at this frequency.
    pub residual_rel: f64,
    /// Row-major `n_ports × n_ports` power-wave S-matrix (see the
    /// [module docs](self) for the normalization): lumped ports first,
    /// then wave channels port-major, mode-minor.
    pub s: Vec<c64>,
    /// Modal `β(ω)` of each wave channel (length `n_ports − n_lumped`,
    /// wave-channel order). `β_im < 0` for evanescent channels.
    pub beta: Vec<c64>,
    /// Total number of S-matrix ports `N_l + Σ_p K_p`.
    pub n_ports: usize,
    /// Number of lumped ports `N_l` (the leading block of `s`).
    pub n_lumped: usize,
    /// Per-wave-port mode count `K_p`.
    pub port_mode_counts: Vec<usize>,
    /// Per-RHS Krylov iteration counts: the `N_w` `A_base⁻¹U` column
    /// solves first, then the `n_ports` excitation solves. All zero on
    /// the direct path.
    pub iters_per_rhs: Vec<usize>,
}

impl MixedPortSweepPoint {
    /// Flat S-matrix index of wave port `port`'s mode `mode`
    /// (`n_lumped + Σ_{q<port} K_q + mode`).
    ///
    /// # Panics
    /// Panics if `port` or `mode` is out of range.
    pub fn wave_channel_index(&self, port: usize, mode: usize) -> usize {
        assert!(port < self.port_mode_counts.len());
        assert!(mode < self.port_mode_counts[port]);
        self.n_lumped + self.port_mode_counts[..port].iter().sum::<usize>() + mode
    }
}

/// Mixed lumped + wave port S-parameter sweep (issue #759) with an
/// explicit [`SolverMode`]. See the [module docs](self) for the
/// formulation, the power-wave normalization and the channel order.
///
/// `lumped` and `wave` may each be empty, but not both. Every lumped port
/// needs a non-zero `v_inc` and every wave mode a non-zero `a_inc` (each
/// is an excitation column). `surfaces` (Leontovich / Silver-Müller walls)
/// are composed into the base operator as on the lumped path.
///
/// # Errors
///
/// [`DrivenError::InvalidPort`] for an empty port set, a zero drive or a
/// mode-length mismatch; [`DrivenError::UnsupportedMatrixFree`] for
/// [`SolverMode::IterativeMatrixFree`] (the SMW post-step needs the
/// assembled `A(ω)`); any [`DrivenError`] from assembly or the per-ω
/// solves.
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_port_sweep_with_mode<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePort],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<MixedPortSweepPoint>, DrivenError> {
    if lumped.is_empty() && wave.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "mixed-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    if matches!(solver_mode, SolverMode::IterativeMatrixFree(_)) {
        return Err(DrivenError::UnsupportedMatrixFree {
            reason: "mixed lumped + wave port sweeps (rank-N SMW modal-Robin) are not wired to \
                     the matrix-free path; use SolverMode::Direct or SolverMode::Iterative"
                .to_string(),
        });
    }
    for (index, port) in lumped.iter().enumerate() {
        if port.v_inc == c64::new(0.0, 0.0) {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "every lumped port needs a non-zero v_inc to serve as an S-parameter \
                         excitation"
                    .to_string(),
            });
        }
    }
    let edges = mesh.edges();
    let n_edges = edges.len();
    validate_driven_surfaces(mesh, "wave port", wave.iter().map(|p| p.faces.as_slice()))?;

    // Wave channels (port-major, mode-minor) with their ω-independent
    // full-length modal fluxes f = S_p · e.
    struct Channel {
        port: usize,
        mode: usize,
        a_inc: c64,
        flux: Vec<f64>,
    }
    let port_mode_counts: Vec<usize> = wave.iter().map(|p| p.modes.len()).collect();
    let n_wave: usize = port_mode_counts.iter().sum();
    let mut channels: Vec<Channel> = Vec::with_capacity(n_wave);
    for (p_idx, port) in wave.iter().enumerate() {
        if port.modes.is_empty() {
            return Err(DrivenError::InvalidPort {
                index: lumped.len() + p_idx,
                reason: "wave port must carry at least one mode".to_string(),
            });
        }
        for (m_idx, m) in port.modes.iter().enumerate() {
            if m.mode.len() != n_edges {
                return Err(DrivenError::InvalidPort {
                    index: lumped.len() + p_idx,
                    reason: format!(
                        "wave-port mode[{m_idx}] profile length {} must match edge count {}",
                        m.mode.len(),
                        n_edges
                    ),
                });
            }
            if m.a_inc == c64::new(0.0, 0.0) {
                return Err(DrivenError::InvalidPort {
                    index: lumped.len() + p_idx,
                    reason: format!(
                        "wave-port mode[{m_idx}] needs a non-zero a_inc to serve as an excitation"
                    ),
                });
            }
            channels.push(Channel {
                port: p_idx,
                mode: m_idx,
                a_inc: m.a_inc,
                flux: assemble_modal_flux(mesh, &port.faces, &m.mode, &edges),
            });
        }
    }
    let n_lumped = lumped.len();
    let n_ports = n_lumped + n_wave;

    // Base operator: volume terms + lumped loads + walls, zero volume
    // source (ports are the only drive).
    let zero_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        lumped,
        surfaces,
        &zero_source,
        device,
    )?;
    let n_int = op.n_interior();
    // Interior-filtered modal fluxes (ω-independent).
    let fluxes_int: Vec<Vec<c64>> = channels
        .iter()
        .map(|c| {
            c.flux
                .iter()
                .zip(bcs.pec_interior_mask.iter())
                .filter_map(|(&v, &keep)| keep.then_some(c64::new(v, 0.0)))
                .collect()
        })
        .collect();
    let dot = |u: &[c64], v: &[c64]| -> c64 {
        u.iter()
            .zip(v.iter())
            .fold(c64::new(0.0, 0.0), |acc, (&a, &b)| acc + a * b)
    };
    let zero = c64::new(0.0, 0.0);

    omegas
        .iter()
        .map(|&omega| {
            let betas: Vec<c64> = channels
                .iter()
                .map(|c| wave[c.port].modes[c.mode].beta(omega))
                .collect();
            let solver = op.prepare_at::<B>(omega, solver_mode, device)?;
            let mut iters_per_rhs = Vec::with_capacity(n_wave + n_ports);

            // A_base⁻¹ U and the capacitance matrix M = Λ⁻¹ + Uᵀ A_base⁻¹ U
            // (Λ = diag(jβ)); a β = 0 channel is decoupled exactly as on
            // the wave path.
            let mut ainv_u: Vec<Vec<c64>> = Vec::with_capacity(n_wave);
            for col in &fluxes_int {
                let mut x = vec![zero; n_int];
                iters_per_rhs.push(solver.back_solve(col, &mut x)?.iters);
                ainv_u.push(x);
            }
            let mut cap = vec![zero; n_wave * n_wave];
            for i in 0..n_wave {
                for j in 0..n_wave {
                    cap[i * n_wave + j] = dot(&fluxes_int[i], &ainv_u[j]);
                }
                if betas[i].norm_sqr() > 0.0 {
                    cap[i * n_wave + i] += c64::new(0.0, -1.0) / betas[i];
                } else {
                    for k in 0..n_wave {
                        cap[i * n_wave + k] = zero;
                        cap[k * n_wave + i] = zero;
                    }
                    cap[i * n_wave + i] = c64::new(1.0, 0.0);
                }
            }
            let cap_inv = invert_complex_dense(&cap, n_wave).ok_or_else(|| {
                DrivenError::Solve(format!(
                    "mixed-port rank-N SMW capacitance matrix singular at ω = {omega}"
                ))
            })?;

            // Power-wave weights: lumped √R_k, wave √β_q / √ω.
            let sqrt_omega = omega.sqrt();
            let sqrt_r: Vec<f64> = (0..n_lumped)
                .map(|k| op.port_resistance(k).sqrt())
                .collect();
            let wave_weight: Vec<c64> = betas.iter().map(|b| b.sqrt() / sqrt_omega).collect();

            let mut s = vec![zero; n_ports * n_ports];
            let mut residual_rel = 0.0_f64;
            for j in 0..n_ports {
                // Excitation RHS (interior) and its incident power wave ã_j.
                let (b, a_tilde) = if j < n_lumped {
                    (
                        op.assemble_b_at(omega, Some(j)),
                        op.port_v_inc(j) / sqrt_r[j],
                    )
                } else {
                    let c = j - n_lumped;
                    let coeff = c64::new(0.0, 2.0) * betas[c] * channels[c].a_inc;
                    (
                        fluxes_int[c].iter().map(|&f| f * coeff).collect::<Vec<_>>(),
                        channels[c].a_inc * wave_weight[c],
                    )
                };

                // SMW: x = A⁻¹b − (A⁻¹U) M⁻¹ Uᵀ A⁻¹b.
                let mut x = vec![zero; n_int];
                iters_per_rhs.push(solver.back_solve(&b, &mut x)?.iters);
                let y: Vec<c64> = fluxes_int.iter().map(|f| dot(f, &x)).collect();
                for i in 0..n_wave {
                    let zi = (0..n_wave).fold(zero, |acc, k| acc + cap_inv[i * n_wave + k] * y[k]);
                    for (xr, &ur) in x.iter_mut().zip(ainv_u[i].iter()) {
                        *xr -= ur * zi;
                    }
                }

                // Residual ‖(A_base + Σ jβ f fᵀ) x − b‖ / ‖b‖.
                let mut ax = vec![zero; n_int];
                solver.spmv_a(&x, &mut ax);
                for (p, f) in fluxes_int.iter().enumerate() {
                    if betas[p].norm_sqr() == 0.0 {
                        continue;
                    }
                    let scaled = c64::new(0.0, 1.0) * betas[p] * dot(f, &x);
                    for (a, &fr) in ax.iter_mut().zip(f.iter()) {
                        *a += fr * scaled;
                    }
                }
                let (res_n2, b_n2) = ax
                    .iter()
                    .zip(b.iter())
                    .fold((0.0_f64, 0.0_f64), |(r, n), (&a, &bb)| {
                        (r + (a - bb).norm_sqr(), n + bb.norm_sqr())
                    });
                if b_n2 > 0.0 {
                    residual_rel = residual_rel.max((res_n2 / b_n2).sqrt());
                }

                // Scatter and read back every port's outgoing power wave.
                let mut e_edges = vec![zero; n_edges];
                let mut it = x.iter();
                for (e, &keep) in e_edges.iter_mut().zip(bcs.pec_interior_mask.iter()) {
                    if keep {
                        *e = *it.next().expect("interior count matches the PEC mask");
                    }
                }
                for k in 0..n_lumped {
                    let v = op.port_voltage(k, &e_edges);
                    let v_out = if k == j { v - op.port_v_inc(k) } else { v };
                    s[k * n_ports + j] = (v_out / sqrt_r[k]) / a_tilde;
                }
                for (q, ch) in channels.iter().enumerate() {
                    let a_q = ch
                        .flux
                        .iter()
                        .zip(e_edges.iter())
                        .fold(zero, |acc, (&f, &e)| acc + e * f);
                    let row = n_lumped + q;
                    let a_out = if row == j { a_q - ch.a_inc } else { a_q };
                    s[row * n_ports + j] = (a_out * wave_weight[q]) / a_tilde;
                }
            }
            Ok(MixedPortSweepPoint {
                omega,
                residual_rel,
                s,
                beta: betas,
                n_ports,
                n_lumped,
                port_mode_counts: port_mode_counts.clone(),
                iters_per_rhs,
            })
        })
        .collect()
}
