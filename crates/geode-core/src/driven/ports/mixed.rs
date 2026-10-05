//! Mixed **lumped + wave** port S-parameter sweep (Epic #756 Phase 8c,
//! issue #759).
//!
//! See [`solve_mixed_port_sweep_with_mode`] for the formulation, the
//! power-wave S normalization and the channel order.

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
    /// Row-major `n_ports × n_ports` power-wave S-matrix (see
    /// [`solve_mixed_port_sweep_with_mode`] for the normalization): lumped ports first,
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
/// explicit [`SolverMode`].
///
/// Lumped ports ([`LumpedPort`]) and wave (modal) ports ([`WavePort`])
/// terminate the same complex-symmetric curl-curl operator `A(ω)`: a
/// lumped port adds the resistive sheet load `(jω/Z_s) S_p` and a wave
/// port adds the rank-`K_p` modal admittance `Σ_m jβ_m f_m f_mᵀ`. This
/// function composes the two in one operator:
///
/// * the lumped loads go into the base operator through
///   [`DrivenOperator::assemble`] (exactly as the lumped-port sweep
///   does), so `A_base(ω) = K − ω²M + jωC + Σ_k (jω/Z_s,k) S_k`;
/// * the modal terms are folded in per frequency by the same rank-`N_w`
///   Sherman-Morrison-Woodbury update as
///   [`crate::driven::ports::solve_wave_port_sweep_with_mode`].
///
/// Each of the `N_l + N_w` excitations is one SMW-corrected solve: a
/// lumped port `j` is driven with its matched-source Thévenin RHS
/// `(2jω/Z_s)(V_inc/l) g_j` (the RHS `solve_excited(j)` of the lumped
/// sweep uses), a wave channel with `2jβ a_inc f_j`. Every other port is a
/// passive matched termination.
///
/// # Power-wave S normalization
///
/// A mixed network has no impedance matrix (a modal channel defines no
/// port voltage), so the lumped sweep's `Z → S` route is not available.
/// The S-matrix is formed directly from power waves. In solver natural
/// units (`η₀ = μ₀ = 1`, `ω = k₀`, `R` in units of η₀) the incident and
/// outgoing amplitudes, normalized so `|ã|²` is the power up to one shared
/// constant, are
///
/// ```text
/// lumped port k (Thévenin 2·V_inc behind R_k):
///     ã_k = V_inc,k / √R_k          b̃_k = (V_k − V_inc,k δ) / √R_k
/// wave channel q (TE modal power |a|²β/(2ωμ), S_p-orthonormal modes):
///     ã_q = a_inc,q · √(β_q/ω)       b̃_q = (a_q − a_inc,q δ) · √(β_q/ω)
/// ```
///
/// and `S_ij = b̃_i / ã_j`. Block by block:
///
/// ```text
/// lumped ← lumped:  S_kj = (V_k − V_inc,j δ_kj) / V_inc,j · √(R_j / R_k)
/// wave   ← wave:    S_qp = (a_q − a_inc,p δ_qp) / a_inc,p · √β_q / √β_p
/// wave   ← lumped:  S_qj = a_q · (√β_q / √ω) · √R_j / V_inc,j
/// lumped ← wave:    S_kp = (V_k / √R_k) / (a_inc,p · √β_p / √ω)
/// ```
///
/// The lumped block is the lumped sweep's real-reference power-wave
/// `F(Z − Z₀)(Z + Z₀)⁻¹F⁻¹` with `Z₀ = diag(R)` (the port's own
/// termination is its reference). The wave block is the wave sweep's
/// power-normalized modal S. The cross weight `√(β/ω)` is fixed by the
/// symmetry `A(ω)ᵀ = A(ω)`: for a lumped RHS `b_l` and a wave RHS `b_w`,
/// `b_wᵀ E⁽ˡ⁾ = b_lᵀ E⁽ʷ⁾` reads `2jβ_q a_inc,q a_q⁽ˡ⁾ = 2jω V_inc,j
/// V_j⁽ʷ⁾ / R_j` (using `g_jᵀ E = w V`, `Z_s = R w / l`), which is exactly
/// `S_qj = S_jq` under the weights above. `√β` is the principal-branch
/// root, as on the wave path, so evanescent channels (`β = −j|β|`) keep
/// `Sᵀ = S`.
///
/// **Filled ports** (issue #777): for a wave port whose
/// [`WavePort::medium`] is not vacuum, every `β` in the drive, the modal
/// term and the weights above is the admittance factor `y = β/μ_t`
/// ([`crate::driven::ports::PortMedium::admittance`]; the TE power is
/// `|a|²·Re β/(2ωμ_t)`), and [`MixedPortSweepPoint::beta`] reports the
/// filled `β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c²`.
///
/// **Caveat.** The modal admittance `jβ` is the TE form (`1/Z_TE =
/// β/ωμ`). The port-face eigensolver filters TEM modes, and TM-mode wave
/// ports are already approximate on the pure-wave path; mixing in lumped
/// ports does not change that.
///
/// # Channel order
///
/// Lumped ports first (input order), then each wave port's modes
/// (port-major, mode-minor): flat index `N_l + Σ_{q<p} K_q + m`.
///
/// The pure-lumped ([`crate::driven::extraction::s_parameter_point`]) and
/// pure-wave ([`crate::driven::ports::solve_wave_port_sweep_with_mode`]) paths are
/// separate functions and are not routed through this one.
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
    let channels = modal_channels(mesh, lumped.len(), wave, &edges)?;
    let port_mode_counts: Vec<usize> = wave.iter().map(|p| p.modes.len()).collect();
    let n_wave = channels.len();
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
    let fluxes_int = interior_fluxes(&channels, bcs.pec_interior_mask);
    let zero = c64::new(0.0, 0.0);

    omegas
        .iter()
        .map(|&omega| {
            // Reported β, and the admittance factor y = β/μ_t (issue
            // #777; `y = β` for a vacuum port) that every operator / power
            // term below uses in place of β.
            let (betas, ys) = channel_admittances(wave, &channels, omega);
            let solver = op.prepare_at::<B>(omega, solver_mode, device)?;
            let mut iters_per_rhs = Vec::with_capacity(n_wave + n_ports);
            let mut back_solve =
                |b: &[c64], x: &mut [c64]| solver.back_solve(b, x).map(|r| r.iters);

            // A_base⁻¹ U and the SMW capacitance matrix.
            let smw = ModalSmw::prepare(
                &fluxes_int,
                &ys,
                n_int,
                omega,
                &mut back_solve,
                &mut iters_per_rhs,
            )?;

            // Power-wave weights: lumped √R_k, wave √y_q / √ω.
            let weights = PowerWeights::new(&op, n_lumped, &ys, omega);

            let mut s = vec![zero; n_ports * n_ports];
            let mut residual_rel = 0.0_f64;
            for j in 0..n_ports {
                // Excitation RHS (interior) and its incident power wave ã_j.
                let b = excitation_rhs(&op, omega, j, n_lumped, &channels, &fluxes_int, &ys);
                let a_tilde = incident_wave(&op, j, n_lumped, &channels, &weights);

                // SMW: x = A⁻¹b − (A⁻¹U) M⁻¹ Uᵀ A⁻¹b.
                let x = smw.solve(&fluxes_int, &b, &mut back_solve, &mut iters_per_rhs)?;

                // Residual ‖(A_base + Σ jβ f fᵀ) x − b‖ / ‖b‖.
                let mut ax = vec![zero; n_int];
                solver.spmv_a(&x, &mut ax);
                for (p, f) in fluxes_int.iter().enumerate() {
                    if ys[p].norm_sqr() == 0.0 {
                        continue;
                    }
                    let scaled = c64::new(0.0, 1.0) * ys[p] * dot_t(f, &x);
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
                s_column(
                    &op,
                    bcs.pec_interior_mask,
                    n_edges,
                    &channels,
                    &weights,
                    j,
                    a_tilde,
                    &x,
                    &mut s,
                );
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

/// One wave channel (port `port`, mode `mode`) of a mixed / modal port
/// set with its incident amplitude and ω-independent **full-length**
/// modal flux `f = S_p · e` (issue #774: shared by the dense mixed sweep
/// and the adaptive PROM).
pub(crate) struct ModalChannel {
    pub(crate) port: usize,
    pub(crate) mode: usize,
    pub(crate) a_inc: c64,
    pub(crate) flux: Vec<f64>,
}

/// Validate the wave ports (faces on the mesh, ≥ 1 mode each, profile
/// lengths, non-zero `a_inc`) and build their channels port-major,
/// mode-minor. `n_lumped` offsets the reported port index.
pub(crate) fn modal_channels(
    mesh: &TetMesh,
    n_lumped: usize,
    wave: &[WavePort],
    edges: &[[u32; 2]],
) -> Result<Vec<ModalChannel>, DrivenError> {
    let n_edges = edges.len();
    validate_driven_surfaces(mesh, "wave port", wave.iter().map(|p| p.faces.as_slice()))?;
    let n_wave: usize = wave.iter().map(|p| p.modes.len()).sum();
    let mut channels: Vec<ModalChannel> = Vec::with_capacity(n_wave);
    for (p_idx, port) in wave.iter().enumerate() {
        if port.modes.is_empty() {
            return Err(DrivenError::InvalidPort {
                index: n_lumped + p_idx,
                reason: "wave port must carry at least one mode".to_string(),
            });
        }
        for (m_idx, m) in port.modes.iter().enumerate() {
            if m.mode.len() != n_edges {
                return Err(DrivenError::InvalidPort {
                    index: n_lumped + p_idx,
                    reason: format!(
                        "wave-port mode[{m_idx}] profile length {} must match edge count {}",
                        m.mode.len(),
                        n_edges
                    ),
                });
            }
            if m.a_inc == c64::new(0.0, 0.0) {
                return Err(DrivenError::InvalidPort {
                    index: n_lumped + p_idx,
                    reason: format!(
                        "wave-port mode[{m_idx}] needs a non-zero a_inc to serve as an excitation"
                    ),
                });
            }
            channels.push(ModalChannel {
                port: p_idx,
                mode: m_idx,
                a_inc: m.a_inc,
                flux: assemble_modal_flux(mesh, &port.faces, &m.mode, edges),
            });
        }
    }
    Ok(channels)
}

/// The channels' modal fluxes restricted to the interior (PEC-masked)
/// edges, as complex vectors.
pub(crate) fn interior_fluxes(
    channels: &[ModalChannel],
    pec_interior_mask: &[bool],
) -> Vec<Vec<c64>> {
    channels
        .iter()
        .map(|c| {
            c.flux
                .iter()
                .zip(pec_interior_mask.iter())
                .filter_map(|(&v, &keep)| keep.then_some(c64::new(v, 0.0)))
                .collect()
        })
        .collect()
}

/// Reported `β` and admittance factor `y = β/μ_t` of every channel at
/// `omega` — the exact calls the dense sweep makes.
pub(crate) fn channel_admittances(
    wave: &[WavePort],
    channels: &[ModalChannel],
    omega: f64,
) -> (Vec<c64>, Vec<c64>) {
    let betas: Vec<c64> = channels
        .iter()
        .map(|c| wave[c.port].beta(c.mode, omega))
        .collect();
    let ys: Vec<c64> = channels
        .iter()
        .zip(&betas)
        .map(|(c, &b)| wave[c.port].medium.admittance(b))
        .collect();
    (betas, ys)
}

/// Unconjugated bilinear form `Σ uᵢ vᵢ` (the complex-symmetric pairing).
pub(crate) fn dot_t(u: &[c64], v: &[c64]) -> c64 {
    u.iter()
        .zip(v.iter())
        .fold(c64::new(0.0, 0.0), |acc, (&a, &b)| acc + a * b)
}

/// A back-solve through some factorization / Krylov solver of `A_base(ω)`
/// returning its iteration count (0 on the direct path).
pub(crate) type BackSolve<'s> = dyn FnMut(&[c64], &mut [c64]) -> Result<usize, DrivenError> + 's;

/// The per-ω rank-`N_w` Sherman-Morrison-Woodbury data folding the modal
/// terms `Σ_q j·y_q f_q f_qᵀ` into solves with `A_base(ω)`: the columns
/// `A_base⁻¹U` and the inverse capacitance matrix
/// `(Λ⁻¹ + Uᵀ A_base⁻¹ U)⁻¹`, `Λ = diag(j·y)`.
pub(crate) struct ModalSmw {
    ainv_u: Vec<Vec<c64>>,
    cap_inv: Vec<c64>,
}

impl ModalSmw {
    /// `N_w` back-solves for `A_base⁻¹U` (iteration counts appended to
    /// `iters`) and the capacitance inverse; a `y = 0` channel is
    /// decoupled exactly as on the wave path.
    pub(crate) fn prepare(
        fluxes_int: &[Vec<c64>],
        ys: &[c64],
        n_int: usize,
        omega: f64,
        back_solve: &mut BackSolve<'_>,
        iters: &mut Vec<usize>,
    ) -> Result<Self, DrivenError> {
        let zero = c64::new(0.0, 0.0);
        let n_wave = fluxes_int.len();
        let mut ainv_u: Vec<Vec<c64>> = Vec::with_capacity(n_wave);
        for col in fluxes_int {
            let mut x = vec![zero; n_int];
            iters.push(back_solve(col, &mut x)?);
            ainv_u.push(x);
        }
        let mut cap = vec![zero; n_wave * n_wave];
        for i in 0..n_wave {
            for j in 0..n_wave {
                cap[i * n_wave + j] = dot_t(&fluxes_int[i], &ainv_u[j]);
            }
            if ys[i].norm_sqr() > 0.0 {
                cap[i * n_wave + i] += c64::new(0.0, -1.0) / ys[i];
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
        Ok(Self { ainv_u, cap_inv })
    }

    /// The SMW-corrected interior solution of `(A_base + Σ j·y f fᵀ) x =
    /// b`: `x = A⁻¹b − (A⁻¹U) M⁻¹ Uᵀ A⁻¹b` (one back-solve, its iteration
    /// count appended to `iters`).
    pub(crate) fn solve(
        &self,
        fluxes_int: &[Vec<c64>],
        b: &[c64],
        back_solve: &mut BackSolve<'_>,
        iters: &mut Vec<usize>,
    ) -> Result<Vec<c64>, DrivenError> {
        let zero = c64::new(0.0, 0.0);
        let n_wave = fluxes_int.len();
        let mut x = vec![zero; b.len()];
        iters.push(back_solve(b, &mut x)?);
        let y: Vec<c64> = fluxes_int.iter().map(|f| dot_t(f, &x)).collect();
        for i in 0..n_wave {
            let zi = (0..n_wave).fold(zero, |acc, k| acc + self.cap_inv[i * n_wave + k] * y[k]);
            for (xr, &ur) in x.iter_mut().zip(self.ainv_u[i].iter()) {
                *xr -= ur * zi;
            }
        }
        Ok(x)
    }
}

/// Power-wave weights at one ω: lumped `√R_k`, wave `√y_q / √ω`.
pub(crate) struct PowerWeights {
    pub(crate) sqrt_r: Vec<f64>,
    pub(crate) wave_weight: Vec<c64>,
}

impl PowerWeights {
    pub(crate) fn new(op: &DrivenOperator, n_lumped: usize, ys: &[c64], omega: f64) -> Self {
        let sqrt_omega = omega.sqrt();
        let sqrt_r: Vec<f64> = (0..n_lumped)
            .map(|k| op.port_resistance(k).sqrt())
            .collect();
        let wave_weight: Vec<c64> = ys.iter().map(|y| y.sqrt() / sqrt_omega).collect();
        Self {
            sqrt_r,
            wave_weight,
        }
    }
}

/// Interior RHS of excitation `j` (flat channel order): a lumped port's
/// matched-source drive `op.assemble_b_at(ω, Some(j))`, or a wave
/// channel's `2j·y·a_inc·f`.
pub(crate) fn excitation_rhs(
    op: &DrivenOperator,
    omega: f64,
    j: usize,
    n_lumped: usize,
    channels: &[ModalChannel],
    fluxes_int: &[Vec<c64>],
    ys: &[c64],
) -> Vec<c64> {
    if j < n_lumped {
        op.assemble_b_at(omega, Some(j))
    } else {
        let c = j - n_lumped;
        let coeff = c64::new(0.0, 2.0) * ys[c] * channels[c].a_inc;
        fluxes_int[c].iter().map(|&f| f * coeff).collect()
    }
}

/// Incident power wave `ã_j` of excitation `j`: `V_inc/√R` (lumped) or
/// `a_inc·√y/√ω` (wave).
pub(crate) fn incident_wave(
    op: &DrivenOperator,
    j: usize,
    n_lumped: usize,
    channels: &[ModalChannel],
    weights: &PowerWeights,
) -> c64 {
    if j < n_lumped {
        op.port_v_inc(j) / weights.sqrt_r[j]
    } else {
        let c = j - n_lumped;
        channels[c].a_inc * weights.wave_weight[c]
    }
}

/// Scatter the interior solution `x` of excitation `j` to the full edge
/// vector and write S-matrix column `j` (row-major `s`, `n_lumped +
/// channels.len()` ports): `S_kj = b̃_k / ã_j`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s_column(
    op: &DrivenOperator,
    pec_interior_mask: &[bool],
    n_edges: usize,
    channels: &[ModalChannel],
    weights: &PowerWeights,
    j: usize,
    a_tilde: c64,
    x: &[c64],
    s: &mut [c64],
) {
    let zero = c64::new(0.0, 0.0);
    // The modal fluxes are zipped against the scattered vector below; a
    // length mismatch would silently truncate (#804).
    assert_eq!(
        pec_interior_mask.len(),
        n_edges,
        "s_column: PEC mask length != n_edges"
    );
    let n_lumped = weights.sqrt_r.len();
    let n_ports = n_lumped + channels.len();
    let mut e_edges = vec![zero; n_edges];
    let mut it = x.iter();
    for (e, &keep) in e_edges.iter_mut().zip(pec_interior_mask.iter()) {
        if keep {
            *e = *it.next().expect("interior count matches the PEC mask");
        }
    }
    for k in 0..n_lumped {
        let v = op.port_voltage(k, &e_edges);
        let v_out = if k == j { v - op.port_v_inc(k) } else { v };
        s[k * n_ports + j] = (v_out / weights.sqrt_r[k]) / a_tilde;
    }
    for (q, ch) in channels.iter().enumerate() {
        assert_eq!(
            ch.flux.len(),
            n_edges,
            "s_column: modal flux length != n_edges"
        );
        let a_q = ch
            .flux
            .iter()
            .zip(e_edges.iter())
            .fold(zero, |acc, (&f, &e)| acc + e * f);
        let row = n_lumped + q;
        let a_out = if row == j { a_q - ch.a_inc } else { a_q };
        s[row * n_ports + j] = (a_out * weights.wave_weight[q]) / a_tilde;
    }
}
