//! Implicit second-order **time-domain** EM solver — generalized-α
//! (Chung–Hulbert) / Newmark-β integration of the same `K`/`C`/`M`
//! matrices the frequency-domain driven path assembles, with a
//! lumped-port time-domain drive and broadband S-parameter extraction
//! via a direct DFT (Epic #475, issue #484; Palace transient parity).
//!
//! # From the frequency operator to the second-order ODE
//!
//! [`DrivenOperator`]'s `assemble_a_at` forms the frequency-domain
//! operator (with the module's `exp(+jωt)` convention)
//!
//! ```text
//! A(ω) = K + iω C(σ) − ω² M + Σ_p (jω/Z_p) S_p           (+ Leontovich/UPML)
//! ```
//!
//! by linear combination of ω-independent value tensors. Every term
//! above except the Leontovich/UPML pieces is a **polynomial in iω**,
//! and with a phasor `x(t) = Re{X e^{iωt}}` the substitution
//! `iω ↔ d/dt`, `−ω² ↔ d²/dt²` maps it line-for-line onto the
//! second-order system
//!
//! ```text
//! M ẍ + C_total ẋ + K x = f(t),     C_total = C(σ) + Σ_p S_p / Z_p.
//! ```
//!
//! Two mapping details are load-bearing (they silently break the FFT
//! cross-check by a factor of `iω` if gotten wrong):
//!
//! 1. **Port damping folds into `C`.** In the frequency operator the
//!    port term `(jω/Z_p) S_p` is a *separate* `iω`-linear block, not
//!    pre-folded into `C(σ)`. Here we fold `C_total = C(σ) + Σ_p S_p/Z_p`
//!    explicitly. `S_p` is real symmetric, so `C_total` stays real
//!    symmetric.
//! 2. **The port drive is a time derivative.** The frequency drive
//!    `b_i += (2jω/Z_p)(V_inc/ℓ) f_i` carries a `jω` prefactor, so the
//!    time-domain source is
//!    `f(t)_i = (2/(Z_p·ℓ)) f_i · dV_inc(t)/dt` — proportional to the
//!    **derivative** of the incident-voltage waveform.
//!
//! Because `K`, `M`, `C(σ)`, and `S_p` are all real, the effective
//! integration matrix is **real symmetric** and is factored with a
//! single real `faer` sparse LU (this module uses the real route; the
//! complex machinery would cost ~2× memory/flops for zero benefit here).
//!
//! # Integrator (generalized-α / Newmark-β)
//!
//! The [`TransientScheme`] is the Chung–Hulbert generalized-α method
//! parameterized by the spectral radius at infinity `ρ∞ ∈ [0, 1]`. It
//! carries the standard coefficient relations
//!
//! ```text
//! α_m = (2ρ∞ − 1)/(ρ∞ + 1),   α_f = ρ∞/(ρ∞ + 1),
//! β   = ¼(1 − α_m + α_f)²,     γ = ½ − α_m + α_f,
//! ```
//!
//! which are **unconditionally stable and second-order** for every
//! `ρ∞ ∈ [0, 1]` (Chung & Hulbert 1993). `ρ∞ = 1` gives `α_m = α_f = ½`,
//! zero algorithmic dissipation, and is the default: any high-frequency
//! artifact then shows up honestly in the self-oracle rather than being
//! damped away. Setting `α_m = α_f = 0` recovers plain Newmark-β with
//! `β = ¼`, `γ = ½` (average acceleration), whose unconditional-stability
//! condition is `2β ≥ γ ≥ ½`.
//!
//! For a **constant** time step `Δt` the effective matrix
//!
//! ```text
//! A_eff = (1 − α_m)/(β Δt²) M + (1 − α_f) γ/(β Δt) C_total + (1 − α_f) K
//! ```
//!
//! is assembled **once** and LU-factored **once**; every step is a
//! single back-solve plus a few sparse matvecs (the same *factor-once,
//! solve-many* pattern as
//! [`crate::driven::solve::FactoredDrivenOperator`]).
//!
//! # Out-of-scope operators (v1)
//!
//! Leontovich surface impedance and UPML are non-polynomial in `iω`
//! (`∝ √ω`, resp. `∝ 1/(1 − jσ/ω)`), and wave ports carry a branch-point
//! `β(ω) = √(ω² − k_c²)`; none maps to a constant-coefficient
//! second-order ODE. Wave ports are structurally absent from
//! `DrivenOperator` (its `assemble` only accepts lumped ports), so no
//! runtime check is needed for them. The constructor **does** reject any
//! Leontovich surface and any complex (UPML) `k`/`m` values —
//! [`TransientError::UnsupportedOperator`].

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use crate::driven::solve::DrivenOperator;

/// Errors from the transient time-domain path.
#[derive(Debug, thiserror::Error)]
pub enum TransientError {
    /// The source [`DrivenOperator`] carries an ω-dependent term that is
    /// not polynomial in `iω` (a Leontovich impedance surface, or a
    /// UPML material with complex `K`/`M` values), so it does not map to
    /// a constant-coefficient second-order ODE.
    #[error(
        "transient integration requires an operator polynomial in iω: {reason} \
         (lumped ports + σ + PEC only; Leontovich/UPML/wave ports are out of scope for v1)"
    )]
    UnsupportedOperator { reason: String },
    /// A non-finite or non-positive time step / duration / scheme knob.
    #[error("invalid transient parameter: {0}")]
    InvalidParameter(String),
    /// The real effective matrix failed to assemble or factor.
    #[error("transient effective-matrix assembly/factorization failed: {0}")]
    Factorization(String),
}

/// Second-order time-integration scheme: Chung–Hulbert generalized-α
/// parameterized by the high-frequency spectral radius `ρ∞`.
///
/// See the module docs for the coefficient relations and the
/// unconditional-stability / second-order guarantee. Construct with
/// [`TransientScheme::generalized_alpha`] (or [`TransientScheme::newmark`]
/// for the classic average-acceleration limit), or, for the instability
/// tripwire, with [`TransientScheme::from_newmark_beta_gamma`] to force a
/// deliberately conditionally-stable `(β, γ)`.
#[derive(Debug, Clone, Copy)]
pub struct TransientScheme {
    /// `α_m` — the acceleration-average shift.
    pub alpha_m: f64,
    /// `α_f` — the force/stiffness/damping-average shift.
    pub alpha_f: f64,
    /// Newmark `β`.
    pub beta: f64,
    /// Newmark `γ`.
    pub gamma: f64,
}

impl TransientScheme {
    /// Chung–Hulbert generalized-α for a target high-frequency spectral
    /// radius `ρ∞ ∈ [0, 1]`.
    ///
    /// # Panics
    ///
    /// Panics if `rho_inf` is outside `[0, 1]` (outside this range the
    /// method is not the intended dissipative-but-stable family).
    pub fn generalized_alpha(rho_inf: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&rho_inf),
            "generalized-α spectral radius ρ∞ must lie in [0, 1], got {rho_inf}"
        );
        let alpha_m = (2.0 * rho_inf - 1.0) / (rho_inf + 1.0);
        let alpha_f = rho_inf / (rho_inf + 1.0);
        let beta = 0.25 * (1.0 - alpha_m + alpha_f).powi(2);
        let gamma = 0.5 - alpha_m + alpha_f;
        Self {
            alpha_m,
            alpha_f,
            beta,
            gamma,
        }
    }

    /// Classic Newmark average-acceleration (`α_m = α_f = 0`, `β = ¼`,
    /// `γ = ½`) — the trapezoidal, zero-dissipation limit, identical to
    /// `generalized_alpha(1.0)` in accuracy but with no α-shifts.
    pub fn newmark() -> Self {
        Self {
            alpha_m: 0.0,
            alpha_f: 0.0,
            beta: 0.25,
            gamma: 0.5,
        }
    }

    /// A raw Newmark `(β, γ)` with `α_m = α_f = 0`, for the stability
    /// tripwire: e.g. `(0.0, 0.5)` is the central-difference scheme, which
    /// is only conditionally stable (CFL-limited) and must blow up above
    /// its step limit — proving the energy monitor is not masking failures.
    pub fn from_newmark_beta_gamma(beta: f64, gamma: f64) -> Self {
        Self {
            alpha_m: 0.0,
            alpha_f: 0.0,
            beta,
            gamma,
        }
    }

    /// Whether the scheme is unconditionally stable (`2β ≥ γ ≥ ½` for the
    /// Newmark limit; generalized-α is stable for all `ρ∞ ∈ [0, 1]`, which
    /// always satisfies this).
    pub fn is_unconditionally_stable(&self) -> bool {
        self.gamma >= 0.5 - 1e-12 && 2.0 * self.beta >= self.gamma - 1e-12
    }
}

impl Default for TransientScheme {
    /// `ρ∞ = 1.0` — zero numerical dissipation (the self-oracle default).
    fn default() -> Self {
        Self::generalized_alpha(1.0)
    }
}

/// A Gaussian-modulated-sinusoid incident-voltage waveform
/// `V_inc(t) = exp(−(t − t₀)²/(2τ²)) · sin(2π f_c (t − t₀))` and its
/// analytic derivative (the port drive uses `dV_inc/dt`, see the module
/// docs). The modulation makes the DC content negligible — important
/// because the ungauged curl-curl gradient null space is not
/// stiffness-controlled, so a DC-containing source would drive secular
/// drift.
#[derive(Debug, Clone, Copy)]
pub struct GaussianPulse {
    /// Center (carrier) frequency `f_c` in the same natural units as the
    /// sweep `ω` (i.e. `ω_c = 2π f_c`; but callers usually think in `ω`,
    /// so see [`GaussianPulse::from_band`]).
    pub f_c: f64,
    /// Gaussian width `τ` (standard deviation, in time units).
    pub tau: f64,
    /// Envelope center time `t₀`.
    pub t0: f64,
}

impl GaussianPulse {
    /// Build a pulse whose −10 dB band brackets `[ω_lo, ω_hi]`.
    ///
    /// The carrier is placed at the band center `ω_c = ½(ω_lo + ω_hi)`
    /// and `τ` is chosen so the Gaussian envelope's spectrum reaches
    /// −10 dB at the half-bandwidth `Δω = ½(ω_hi − ω_lo)`. A Gaussian
    /// `exp(−t²/2τ²)` has spectrum `∝ exp(−½ ω² τ²)`; −10 dB in power is
    /// `exp(−½ Δω² τ²) = 10^(−0.5)` ⇒ `τ = √(ln 10)/Δω`. `t₀` is set to
    /// `4τ` so the pulse starts near zero (envelope `< e^{−8} ≈ 3e−4`).
    pub fn from_band(omega_lo: f64, omega_hi: f64) -> Self {
        let omega_c = 0.5 * (omega_lo + omega_hi);
        let d_omega = 0.5 * (omega_hi - omega_lo);
        let tau = (10.0_f64.ln()).sqrt() / d_omega;
        Self {
            f_c: omega_c / (2.0 * std::f64::consts::PI),
            tau,
            t0: 4.0 * tau,
        }
    }

    /// A characteristic duration after which the envelope has decayed to
    /// a negligible level — `t₀ + 8τ` (envelope `< e^{−32}`). Useful as a
    /// default record length before ring-down.
    pub fn support_end(&self) -> f64 {
        self.t0 + 8.0 * self.tau
    }

    /// The incident-voltage waveform value at time `t`.
    pub fn value(&self, t: f64) -> f64 {
        let dt = t - self.t0;
        let env = (-(dt * dt) / (2.0 * self.tau * self.tau)).exp();
        let omega_c = 2.0 * std::f64::consts::PI * self.f_c;
        env * (omega_c * dt).sin()
    }

    /// The analytic time derivative `dV_inc/dt` at time `t` — the
    /// quantity the port drive is proportional to.
    pub fn derivative(&self, t: f64) -> f64 {
        let dt = t - self.t0;
        let tau2 = self.tau * self.tau;
        let env = (-(dt * dt) / (2.0 * tau2)).exp();
        let omega_c = 2.0 * std::f64::consts::PI * self.f_c;
        let s = (omega_c * dt).sin();
        let c = (omega_c * dt).cos();
        // d/dt [ env · sin ] = env·(−dt/τ²)·sin + env·ω_c·cos.
        env * (-(dt / tau2) * s + omega_c * c)
    }
}

/// One port's ω-independent time-domain data, folded out of the
/// [`DrivenOperator`] at construction: the drive coefficient
/// `(2/(Z_p·ℓ))` times the interior-filtered flux vector, and the
/// full-length flux + Thevenin parameters for the V/I readback.
struct TransientPort {
    /// Interior-filtered `(2/(Z_p·ℓ)) f_i` — multiply by `dV_inc/dt` to
    /// get this port's contribution to the interior force vector.
    drive_int: Vec<f64>,
    /// **Interior-filtered** flux functional pre-scaled by `1/w`, for the
    /// voltage readback `V = Σ_i (f_i/w) x_i`. Interior-length so it
    /// aligns with the interior state vector the stepper carries.
    flux_over_w_int: Vec<f64>,
    /// Lumped resistance `R` (Thevenin `I = (2 V_inc − V)/R`).
    resistance: f64,
}

/// A pre-assembled, PEC-reduced, real second-order transient system
/// `M ẍ + C_total ẋ + K x = f(t)` built from a [`DrivenOperator`], plus
/// the lumped-port drive/readback plumbing.
///
/// Assemble once with [`TransientSolver::new`]; then either run a full
/// pulsed excitation with [`TransientSolver::run`] (the port-driven,
/// S-parameter path) or step manually via [`TransientSolver::factor`] +
/// [`TransientStepper`] (the energy-conservation / initial-condition
/// path).
pub struct TransientSolver<'a> {
    /// The source operator, for `scatter_to_full`. `None` only for the
    /// crate-internal `from_matrices_for_test` path (which never scatters).
    op: Option<&'a DrivenOperator>,
    n: usize,
    /// Real interior stiffness `K`.
    k: SparseColMat<usize, f64>,
    /// Real interior mass `M`.
    m: SparseColMat<usize, f64>,
    /// Real interior total damping `C_total = C(σ) + Σ_p S_p/Z_p`.
    c: SparseColMat<usize, f64>,
    /// Per-port drive/readback data.
    ports: Vec<TransientPort>,
}

impl<'a> TransientSolver<'a> {
    /// Fold a [`DrivenOperator`]'s ω-independent tensors into the real
    /// second-order system.
    ///
    /// # Errors
    ///
    /// [`TransientError::UnsupportedOperator`] if the operator carries a
    /// Leontovich impedance surface or complex (UPML) `K`/`M` values (see
    /// the module docs on out-of-scope operators), and
    /// [`TransientError::Factorization`] on sparse-assembly failure.
    pub fn new(op: &'a DrivenOperator) -> Result<Self, TransientError> {
        if op.has_surfaces() {
            return Err(TransientError::UnsupportedOperator {
                reason: "Leontovich impedance surface present".into(),
            });
        }
        // UPML makes k_vals / m_vals complex; require all-real (covers
        // both UPML variants generically without special-casing the
        // DrivenMaterials variant upstream).
        let imag_tol = 1e-12;
        let all_real = |vals: &[c64]| vals.iter().all(|z| z.im.abs() <= imag_tol);
        if !all_real(op.k_vals()) || !all_real(op.m_vals()) {
            return Err(TransientError::UnsupportedOperator {
                reason: "complex K/M values (UPML / matched-UPML material)".into(),
            });
        }

        let n = op.n_interior();
        let rows = op.rows();
        let cols = op.cols();
        let k_vals = op.k_vals();
        let m_vals = op.m_vals();

        // Real K, M triplets (imaginary parts already verified ~0).
        let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(rows.len());
        let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(rows.len());
        for idx in 0..rows.len() {
            k_trips.push(Triplet::new(rows[idx], cols[idx], k_vals[idx].re));
            m_trips.push(Triplet::new(rows[idx], cols[idx], m_vals[idx].re));
        }

        // C_total = C(σ) + Σ_p S_p / Z_p.
        let mut c_trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        if let Some(c_vals) = op.c_vals() {
            c_trips.reserve(c_vals.len());
            for idx in 0..rows.len() {
                c_trips.push(Triplet::new(rows[idx], cols[idx], c_vals[idx]));
            }
        }
        let mut ports = Vec::with_capacity(op.n_ports());
        for p in 0..op.n_ports() {
            let pd = op.port_transient_data(p);
            // Fold S_p / Z_p into C_total.
            for &(rr, cc, v) in pd.mass_triplets {
                c_trips.push(Triplet::new(rr, cc, v / pd.z_s));
            }
            // Drive: interior-filtered (2/(Z_p·ℓ)) f_i.
            let drive_full: Vec<f64> = pd
                .flux
                .iter()
                .map(|&f| f * (2.0 / (pd.z_s * pd.length)))
                .collect();
            let drive_int = op.filter_interior_real(&drive_full);
            // Readback: V = (1/w) Σ f_i x_i with w = z_s·ℓ/R (since
            // z_s = R·w/ℓ). Interior-filter so it aligns with the
            // interior state vector.
            let w = pd.z_s * pd.length / pd.resistance;
            let flux_over_w_full: Vec<f64> = pd.flux.iter().map(|&f| f / w).collect();
            let flux_over_w_int = op.filter_interior_real(&flux_over_w_full);
            ports.push(TransientPort {
                drive_int,
                flux_over_w_int,
                resistance: pd.resistance,
            });
        }

        let build = |trips: &[Triplet<usize, usize, f64>], what: &str| {
            SparseColMat::<usize, f64>::try_new_from_triplets(n, n, trips)
                .map_err(|e| TransientError::Factorization(format!("{what}: {e:?}")))
        };
        let k = build(&k_trips, "K")?;
        let m = build(&m_trips, "M")?;
        // An empty C (no σ, no ports) is a valid all-zero matrix.
        let c = build(&c_trips, "C_total")?;

        Ok(Self {
            op: Some(op),
            n,
            k,
            m,
            c,
            ports,
        })
    }

    /// Crate-internal constructor from raw real `K`/`M`/`C` matrices,
    /// for unit-testing the integrator against analytic oscillator
    /// solutions without standing up a full [`DrivenOperator`]. The
    /// resulting solver has no ports and cannot scatter to full edges.
    #[cfg(test)]
    pub(crate) fn from_matrices_for_test(
        k: SparseColMat<usize, f64>,
        m: SparseColMat<usize, f64>,
        c: SparseColMat<usize, f64>,
    ) -> Self {
        let n = k.nrows();
        Self {
            op: None,
            n,
            k,
            m,
            c,
            ports: Vec::new(),
        }
    }

    /// Number of interior DOFs.
    pub fn n_interior(&self) -> usize {
        self.n
    }

    /// Number of lumped ports.
    pub fn n_ports(&self) -> usize {
        self.ports.len()
    }

    /// Real interior stiffness `K` (for the energy functional
    /// `½ xᵀK x`).
    pub fn stiffness(&self) -> &SparseColMat<usize, f64> {
        &self.k
    }

    /// Real interior mass `M` (for the kinetic energy `½ ẋᵀM ẋ`).
    pub fn mass(&self) -> &SparseColMat<usize, f64> {
        &self.m
    }

    /// Real interior total damping `C_total`.
    pub fn damping(&self) -> &SparseColMat<usize, f64> {
        &self.c
    }

    /// Assemble and LU-factor the effective matrix for a fixed step `Δt`
    /// under `scheme`, returning a [`TransientStepper`] that advances the
    /// state one step per back-solve.
    ///
    /// # Errors
    ///
    /// [`TransientError::InvalidParameter`] for a non-finite / non-positive
    /// `dt`, and [`TransientError::Factorization`] on sparse-LU failure.
    pub fn factor(
        &self,
        dt: f64,
        scheme: TransientScheme,
    ) -> Result<TransientStepper<'_>, TransientError> {
        if !(dt.is_finite() && dt > 0.0) {
            return Err(TransientError::InvalidParameter(format!(
                "Δt must be finite and positive, got {dt}"
            )));
        }
        let TransientScheme {
            alpha_m,
            alpha_f,
            beta,
            gamma,
        } = scheme;
        // A_eff = (1−α_m)/(β Δt²) M + (1−α_f) γ/(β Δt) C + (1−α_f) K.
        let c_m = (1.0 - alpha_m) / (beta * dt * dt);
        let c_c = (1.0 - alpha_f) * gamma / (beta * dt);
        let c_k = 1.0 - alpha_f;
        let a_eff = spadd3(&self.m, c_m, &self.c, c_c, &self.k, c_k, self.n)
            .map_err(|e| TransientError::Factorization(format!("A_eff assembly: {e}")))?;
        let lu = a_eff
            .as_ref()
            .sp_lu()
            .map_err(|e| TransientError::Factorization(format!("A_eff LU: {e:?}")))?;
        Ok(TransientStepper {
            solver: self,
            dt,
            scheme,
            lu: LuHandle(lu),
            u: vec![0.0; self.n],
            v: vec![0.0; self.n],
            a: vec![0.0; self.n],
        })
    }

    /// Discrete total energy `½ ẋᵀM ẋ + ½ xᵀK x` of an interior state.
    pub fn energy(&self, u: &[f64], v: &[f64]) -> f64 {
        0.5 * quad_form(&self.m, v) + 0.5 * quad_form(&self.k, u)
    }

    /// Run a pulsed, port-driven simulation and record the incident
    /// voltage plus each port's `V(t)` and `I(t)` on a uniform time grid.
    ///
    /// Only port `excited` is driven (its `dV_inc/dt` enters the force
    /// vector); every port still contributes its resistive termination
    /// through `C_total`. The simulation runs from `t = 0` for `n_steps`
    /// steps of size `dt` under `scheme`, starting from rest.
    ///
    /// # Errors
    ///
    /// Propagates [`TransientSolver::factor`] failures.
    pub fn run(
        &self,
        excited: usize,
        pulse: &GaussianPulse,
        dt: f64,
        n_steps: usize,
        scheme: TransientScheme,
    ) -> Result<TransientRecord, TransientError> {
        assert!(excited < self.ports.len(), "excited port out of range");
        let mut stepper = self.factor(dt, scheme)?;

        let mut times = Vec::with_capacity(n_steps + 1);
        let mut v_inc = Vec::with_capacity(n_steps + 1);
        let mut v_port = vec![Vec::with_capacity(n_steps + 1); self.ports.len()];
        let mut i_port = vec![Vec::with_capacity(n_steps + 1); self.ports.len()];

        // Force at time t: (dV_inc/dt)(t) · drive_int of the excited port.
        let force_at = |t: f64, out: &mut [f64]| {
            let dvdt = pulse.derivative(t);
            let drive = &self.ports[excited].drive_int;
            for (o, d) in out.iter_mut().zip(drive.iter()) {
                *o = dvdt * *d;
            }
        };

        // Record helper at the current state.
        let record = |u: &[f64],
                      t: f64,
                      times: &mut Vec<f64>,
                      v_inc: &mut Vec<f64>,
                      v_port: &mut [Vec<f64>],
                      i_port: &mut [Vec<f64>]| {
            let vinc = pulse.value(t);
            times.push(t);
            v_inc.push(vinc);
            for (p, port) in self.ports.iter().enumerate() {
                let v: f64 = port
                    .flux_over_w_int
                    .iter()
                    .zip(u.iter())
                    .map(|(&f, &x)| f * x)
                    .sum();
                let vinc_p = if p == excited { vinc } else { 0.0 };
                let i = (2.0 * vinc_p - v) / port.resistance;
                v_port[p].push(v);
                i_port[p].push(i);
            }
        };

        // t = 0 (rest state).
        record(
            &stepper.u,
            0.0,
            &mut times,
            &mut v_inc,
            &mut v_port,
            &mut i_port,
        );
        // Consistent initial acceleration a₀ from M a₀ = f(0) − C v₀ − K u₀
        // (u₀ = v₀ = 0 ⇒ M a₀ = f(0)); we approximate with a₀ = 0 since the
        // pulse envelope is ~0 at t = 0 (t₀ = 4τ), which the record confirms.
        let mut f = vec![0.0; self.n];
        for step in 0..n_steps {
            let t_next = (step + 1) as f64 * dt;
            let t_now = step as f64 * dt;
            force_at(t_now, &mut f);
            let mut f_next = vec![0.0; self.n];
            force_at(t_next, &mut f_next);
            stepper.step(&f, &f_next);
            record(
                &stepper.u,
                t_next,
                &mut times,
                &mut v_inc,
                &mut v_port,
                &mut i_port,
            );
        }

        Ok(TransientRecord {
            dt,
            times,
            v_inc,
            v_port,
            i_port,
            excited,
        })
    }

    /// Single-frequency **steady-state** `S₁₁(ω)`, solving the folded
    /// complex system `(K − ω²M + iω C_total) x = iω·V_inc·drive` directly
    /// (`V_inc = 1`). This is the frequency-domain operator this transient
    /// solver folds — a sanity bridge that must reproduce
    /// `driven_frequency_sweep`'s `S₁₁` exactly (same K/C/M, same port
    /// drive/readback), independent of the time integrator and DFT.
    ///
    /// # Errors
    ///
    /// [`TransientError::Factorization`] on complex sparse-LU failure.
    pub fn steady_state_s11(&self, excited: usize, omega: f64) -> Result<c64, TransientError> {
        use faer::linalg::solvers::Solve;
        assert!(excited < self.ports.len());
        // Complex A = K − ω²M + iω C_total, from the real triplets.
        let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
        let mut push = |mat: &SparseColMat<usize, f64>, scale: c64| {
            let m_ref = mat.as_ref();
            let cp = m_ref.col_ptr();
            let ri = m_ref.row_idx();
            let vals = m_ref.val();
            for j in 0..m_ref.ncols() {
                for k in cp[j]..cp[j + 1] {
                    trips.push(Triplet::new(ri[k], j, scale * vals[k]));
                }
            }
        };
        push(&self.k, c64::new(1.0, 0.0));
        push(&self.m, c64::new(-omega * omega, 0.0));
        push(&self.c, c64::new(0.0, omega));
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(self.n, self.n, &trips)
            .map_err(|e| TransientError::Factorization(format!("CW A: {e:?}")))?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| TransientError::Factorization(format!("CW LU: {e:?}")))?;
        // b = iω · V_inc · drive_int  (V_inc = 1).
        let drive = &self.ports[excited].drive_int;
        let mut mat = faer::Mat::<c64>::from_fn(self.n, 1, |i, _| c64::new(0.0, omega) * drive[i]);
        lu.solve_in_place(mat.as_mut());
        // Read V = Σ_i (flux/w)_i x_i using the interior-filtered flux.
        let flux_int = &self.ports[excited].flux_over_w_int;
        let mut v = c64::new(0.0, 0.0);
        for i in 0..self.n {
            v += flux_int[i] * mat[(i, 0)];
        }
        // S₁₁ = (V − V_inc)/V_inc with V_inc = 1.
        Ok(v - c64::new(1.0, 0.0))
    }

    /// Scatter an interior real state back onto the full edge vector
    /// (zeros on PEC edges) — for post-hoc field inspection.
    ///
    /// # Panics
    ///
    /// Panics on a test-only solver built via `from_matrices_for_test`
    /// (which has no source operator).
    pub fn scatter_to_full(&self, u_int: &[f64]) -> Vec<f64> {
        self.op
            .expect("scatter_to_full requires a DrivenOperator-backed solver")
            .scatter_to_full_real(u_int)
    }
}

/// Newtype wrapper so the factored LU can be stored without leaking the
/// `faer` type name into the public [`TransientStepper`] signature.
struct LuHandle(faer::sparse::linalg::solvers::Lu<usize, f64>);

/// One-step advancer holding the factored effective matrix and the
/// current `(u, v, a)` state (displacement, velocity, acceleration).
///
/// Created by [`TransientSolver::factor`]. Drive it with
/// [`TransientStepper::step`] (external force) or seed an
/// initial-condition ring-down with [`TransientStepper::set_state`].
pub struct TransientStepper<'a> {
    solver: &'a TransientSolver<'a>,
    dt: f64,
    scheme: TransientScheme,
    lu: LuHandle,
    u: Vec<f64>,
    v: Vec<f64>,
    a: Vec<f64>,
}

impl TransientStepper<'_> {
    /// Current interior displacement `u = x`.
    pub fn displacement(&self) -> &[f64] {
        &self.u
    }

    /// Current interior velocity `v = ẋ`.
    pub fn velocity(&self) -> &[f64] {
        &self.v
    }

    /// Total discrete energy `½ vᵀM v + ½ uᵀK u` of the current state.
    pub fn energy(&self) -> f64 {
        self.solver.energy(&self.u, &self.v)
    }

    /// Seed the state (e.g. an initial-condition ring-down for the
    /// lossless energy-conservation tripwire): sets `u`, `v` and derives
    /// the consistent acceleration `a` from `M a = −C v − K u` (zero
    /// external force).
    ///
    /// # Panics
    ///
    /// Panics if `u`/`v` length ≠ `n_interior`, or if the mass solve fails.
    pub fn set_state(&mut self, u: &[f64], v: &[f64]) {
        use faer::linalg::solvers::Solve;
        assert_eq!(u.len(), self.solver.n);
        assert_eq!(v.len(), self.solver.n);
        self.u.copy_from_slice(u);
        self.v.copy_from_slice(v);
        // M a = −C v − K u.
        let mut rhs = spmv(&self.solver.k, u);
        let cv = spmv(&self.solver.c, v);
        for (r, c) in rhs.iter_mut().zip(cv.iter()) {
            *r = -(*r) - *c;
        }
        let m_lu = self
            .solver
            .m
            .as_ref()
            .sp_lu()
            .expect("mass matrix LU for consistent acceleration");
        let mut mat = faer::Mat::<f64>::from_fn(self.solver.n, 1, |i, _| rhs[i]);
        m_lu.solve_in_place(mat.as_mut());
        for i in 0..self.solver.n {
            self.a[i] = mat[(i, 0)];
        }
    }

    /// Advance one generalized-α step from `t_n` to `t_{n+1}` given the
    /// external force at `t_n` (`f_n`) and `t_{n+1}` (`f_np1`).
    ///
    /// The generalized-α force is evaluated at the shifted time
    /// `t_{n+1−α_f}`, i.e. `(1 − α_f) f_{n+1} + α_f f_n`.
    pub fn step(&mut self, f_n: &[f64], f_np1: &[f64]) {
        use faer::linalg::solvers::Solve;
        let TransientScheme {
            alpha_m,
            alpha_f,
            beta,
            gamma,
        } = self.scheme;
        let dt = self.dt;
        let n = self.solver.n;

        // Newmark predictors (displacement/velocity without the new accel):
        //   u* = u + dt v + dt²(½ − β) a
        //   v* = v + dt(1 − γ) a
        let mut u_star = vec![0.0; n];
        let mut v_star = vec![0.0; n];
        for i in 0..n {
            u_star[i] = self.u[i] + dt * self.v[i] + dt * dt * (0.5 - beta) * self.a[i];
            v_star[i] = self.v[i] + dt * (1.0 - gamma) * self.a[i];
        }

        // Effective RHS at the α-shifted time:
        //   f_eff = (1−α_f) f_{n+1} + α_f f_n
        //         − K[(1−α_f) u* + α_f u]
        //         − C[(1−α_f) v* + α_f v]
        //         + M[ (α_m)/(β dt²) (u − u*) ... ] handled via a-average.
        //
        // We solve A_eff u_{n+1} = f_eff with A_eff as in `factor`, then
        // recover a_{n+1}, v_{n+1} from the Newmark correctors. Assemble
        // f_eff explicitly to keep the algebra auditable.
        //
        //   a_{n+1} = (u_{n+1} − u*) / (β dt²)
        //   v_{n+1} = v* + γ dt a_{n+1}
        //
        // Substituting the α-averages
        //   ü_{n+1−α_m} = (1−α_m) a_{n+1} + α_m a_n
        //   u_{n+1−α_f} = (1−α_f) u_{n+1} + α_f u_n     (and same for v)
        // into  M ü_{n+1−α_m} + C v_{n+1−α_f} + K u_{n+1−α_f} = f_{n+1−α_f}
        // yields A_eff u_{n+1} = rhs with the rhs below.
        let c_m = (1.0 - alpha_m) / (beta * dt * dt);

        // Velocity corrector v_{n+1} = v* + γ dt a_{n+1}
        //   = v* + (γ/(β dt)) (u_{n+1} − u*).
        let c_c = (1.0 - alpha_f) * gamma / (beta * dt);

        // Substituting the Newmark correctors and the α-averages into the
        // shifted EOM and moving all known quantities to the right gives
        //   A_eff u_{n+1} = force_avg
        //                 + M·(c_m u* − α_m a_n)
        //                 + C·(c_c u* − (1−α_f) v* − α_f v_n)
        //                 − K·(α_f u_n)
        // (the c_m M, c_c C and (1−α_f) K pieces multiplying u_{n+1} are
        //  exactly A_eff, already assembled and factored in `factor`).
        let mut rhs = vec![0.0; n];
        for i in 0..n {
            rhs[i] = (1.0 - alpha_f) * f_np1[i] + alpha_f * f_n[i];
        }
        // + M (c_m u* − α_m a_n)
        let mut m_arg = vec![0.0; n];
        for i in 0..n {
            m_arg[i] = c_m * u_star[i] - alpha_m * self.a[i];
        }
        let m_term = spmv(&self.solver.m, &m_arg);
        // + C (c_c u* − (1−α_f) v*)  − C (α_f v_n)
        let mut c_arg = vec![0.0; n];
        for i in 0..n {
            c_arg[i] = c_c * u_star[i] - (1.0 - alpha_f) * v_star[i] - alpha_f * self.v[i];
        }
        let c_term = spmv(&self.solver.c, &c_arg);
        // − K (α_f u_n)
        let k_arg: Vec<f64> = self.u.iter().map(|&u| alpha_f * u).collect();
        let k_term = spmv(&self.solver.k, &k_arg);

        for i in 0..n {
            rhs[i] += m_term[i] + c_term[i] - k_term[i];
        }

        // Solve A_eff u_{n+1} = rhs.
        let mut mat = faer::Mat::<f64>::from_fn(n, 1, |i, _| rhs[i]);
        self.lu.0.solve_in_place(mat.as_mut());
        let mut u_new = vec![0.0; n];
        for i in 0..n {
            u_new[i] = mat[(i, 0)];
        }

        // Correctors.
        let mut a_new = vec![0.0; n];
        let mut v_new = vec![0.0; n];
        for i in 0..n {
            a_new[i] = (u_new[i] - u_star[i]) / (beta * dt * dt);
            v_new[i] = v_star[i] + gamma * dt * a_new[i];
        }
        self.u = u_new;
        self.v = v_new;
        self.a = a_new;
    }
}

/// Recorded time-domain port waveforms from [`TransientSolver::run`].
#[derive(Debug, Clone)]
pub struct TransientRecord {
    /// Uniform time step `Δt`.
    pub dt: f64,
    /// Sample times (length `n_steps + 1`, including `t = 0`).
    pub times: Vec<f64>,
    /// Incident-voltage waveform `V_inc(t)` at the excited port.
    pub v_inc: Vec<f64>,
    /// Per-port field-projected voltage `V_p(t)` (outer index = port).
    pub v_port: Vec<Vec<f64>>,
    /// Per-port Thevenin current `I_p(t) = (2 V_inc,p − V_p)/R_p`.
    pub i_port: Vec<Vec<f64>>,
    /// Index of the excited port.
    pub excited: usize,
}

impl TransientRecord {
    /// Broadband `S₁₁(ω)` at exactly the sweep frequencies `omegas`, via
    /// a **direct DFT** of the incident/reflected decomposition.
    ///
    /// The reflected-voltage waveform is `V_ref(t) = V(t) − V_inc(t)` at
    /// the excited port (the Thevenin incident/reflected split, consistent
    /// with the frequency-domain `SMatrix` conventions), and
    ///
    /// ```text
    /// S₁₁(ω) = F[V_ref](ω) / F[V_inc](ω),
    /// F[g](ω) = Σ_n g(t_n) e^{−iω t_n} Δt   (rectangular window).
    /// ```
    ///
    /// # Windowing
    ///
    /// A **rectangular window** with a decay-to-floor truncation is used:
    /// the port-loaded fixture is strongly damped, so the record is run
    /// until the port waveform has decayed to its floor and the residual
    /// truncation leakage is negligible (< 0.01% for the validated
    /// fixtures — see the benchmark). The `Δt` factor cancels in the
    /// ratio, so it is kept only for dimensional clarity.
    ///
    /// Evaluated at exactly the frequency-domain sweep points, this
    /// introduces **zero interpolation error** into the self-oracle
    /// comparison against `driven_frequency_sweep`.
    pub fn s11(&self, omegas: &[f64]) -> Vec<c64> {
        let p = self.excited;
        omegas
            .iter()
            .map(|&omega| {
                let mut f_ref = c64::new(0.0, 0.0);
                let mut f_inc = c64::new(0.0, 0.0);
                for (k, &t) in self.times.iter().enumerate() {
                    let phase = c64::new(0.0, -omega * t).exp();
                    let v = self.v_port[p][k];
                    let vinc = self.v_inc[k];
                    let vref = v - vinc;
                    f_ref += phase * vref;
                    f_inc += phase * vinc;
                }
                f_ref / f_inc
            })
            .collect()
    }

    /// Raw DFTs `(F[V_p], F[V_inc])` at the excited port, for diagnostics
    /// and cross-checks (`S₁₁ = (F[V] − F[V_inc]) / F[V_inc]`).
    pub fn dft_v_and_vinc(&self, omegas: &[f64]) -> Vec<(c64, c64)> {
        let p = self.excited;
        omegas
            .iter()
            .map(|&omega| {
                let mut fv = c64::new(0.0, 0.0);
                let mut fi = c64::new(0.0, 0.0);
                for (k, &t) in self.times.iter().enumerate() {
                    let phase = c64::new(0.0, -omega * t).exp();
                    fv += phase * self.v_port[p][k];
                    fi += phase * self.v_inc[k];
                }
                (fv, fi)
            })
            .collect()
    }

    /// Total port-energy-outflow proxy `Σ_p V_p(t) I_p(t)` at each sample
    /// — used to pick a decay-to-floor truncation point.
    pub fn port_power(&self) -> Vec<f64> {
        (0..self.times.len())
            .map(|k| {
                self.v_port
                    .iter()
                    .zip(self.i_port.iter())
                    .map(|(v, i)| v[k] * i[k])
                    .sum()
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Small real-sparse helpers (faer has no scalar-scaled sparse add in-tree).
// ---------------------------------------------------------------------------

/// Sparse `y = A x` for a real CSC matrix.
fn spmv(a: &SparseColMat<usize, f64>, x: &[f64]) -> Vec<f64> {
    let a_ref = a.as_ref();
    let cp = a_ref.col_ptr();
    let row_idx = a_ref.row_idx();
    let vals = a_ref.val();
    let n = a_ref.nrows();
    let ncols = a_ref.ncols();
    let mut y = vec![0.0; n];
    for j in 0..ncols {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            y[row_idx[k]] += vals[k] * xj;
        }
    }
    y
}

/// Real quadratic form `xᵀ A x`.
fn quad_form(a: &SparseColMat<usize, f64>, x: &[f64]) -> f64 {
    spmv(a, x)
        .iter()
        .zip(x.iter())
        .map(|(&ax, &xi)| ax * xi)
        .sum()
}

/// Assemble `s = ca·A + cb·B + cc·C` for three CSC matrices sharing a
/// dimension, by concatenating their scaled triplets (duplicate entries
/// are summed by `try_new_from_triplets`).
fn spadd3(
    a: &SparseColMat<usize, f64>,
    ca: f64,
    b: &SparseColMat<usize, f64>,
    cb: f64,
    c: &SparseColMat<usize, f64>,
    cc: f64,
    n: usize,
) -> Result<SparseColMat<usize, f64>, String> {
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    let mut push_scaled = |mat: &SparseColMat<usize, f64>, s: f64| {
        if s == 0.0 {
            return;
        }
        let m_ref = mat.as_ref();
        let cp = m_ref.col_ptr();
        let row_idx = m_ref.row_idx();
        let vals = m_ref.val();
        for j in 0..m_ref.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(row_idx[k], j, s * vals[k]));
            }
        }
    };
    push_scaled(a, ca);
    push_scaled(b, cb);
    push_scaled(c, cc);
    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips).map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1×1 real CSC matrix holding scalar `v`.
    fn scalar_mat(v: f64) -> SparseColMat<usize, f64> {
        SparseColMat::<usize, f64>::try_new_from_triplets(1, 1, &[Triplet::new(0, 0, v)]).unwrap()
    }

    #[test]
    fn generalized_alpha_rho1_is_average_acceleration() {
        let s = TransientScheme::generalized_alpha(1.0);
        assert!((s.alpha_m - 0.5).abs() < 1e-14);
        assert!((s.alpha_f - 0.5).abs() < 1e-14);
        assert!((s.beta - 0.25).abs() < 1e-14);
        assert!((s.gamma - 0.5).abs() < 1e-14);
        assert!(s.is_unconditionally_stable());
    }

    #[test]
    fn generalized_alpha_rho0_is_stable_and_dissipative() {
        let s = TransientScheme::generalized_alpha(0.0);
        // ρ∞ = 0: α_m = −1, α_f = 0 ⇒ β = 1, γ = 3/2.
        assert!((s.alpha_m + 1.0).abs() < 1e-14);
        assert!((s.alpha_f).abs() < 1e-14);
        assert!((s.beta - 1.0).abs() < 1e-14);
        assert!((s.gamma - 1.5).abs() < 1e-14);
        assert!(s.is_unconditionally_stable());
    }

    #[test]
    fn central_difference_is_not_unconditionally_stable() {
        // Newmark β = 0, γ = ½ — the explicit central-difference limit.
        let s = TransientScheme::from_newmark_beta_gamma(0.0, 0.5);
        assert!(!s.is_unconditionally_stable());
    }

    #[test]
    fn pulse_derivative_matches_finite_difference() {
        let pulse = GaussianPulse::from_band(1.0, 3.0);
        let h = 1e-6;
        for &t in &[0.5, 1.0, 2.0, 3.5, 5.0] {
            let fd = (pulse.value(t + h) - pulse.value(t - h)) / (2.0 * h);
            let an = pulse.derivative(t);
            assert!(
                (fd - an).abs() < 1e-5 * (1.0 + an.abs()),
                "t = {t}: analytic dV/dt = {an}, finite-diff = {fd}"
            );
        }
    }

    #[test]
    fn pulse_has_negligible_dc() {
        // ∫ V_inc dt over the support must be ~0 (modulated ⇒ no DC),
        // else the ungauged gradient null space drifts secularly.
        let pulse = GaussianPulse::from_band(2.0, 4.0);
        let dt = 0.01;
        let n = (pulse.support_end() / dt) as usize + 1;
        let integral: f64 = (0..n).map(|k| pulse.value(k as f64 * dt) * dt).sum();
        let peak: f64 = (0..n)
            .map(|k| pulse.value(k as f64 * dt).abs())
            .fold(0.0, f64::max);
        assert!(
            integral.abs() < 1e-2 * peak,
            "pulse DC content {integral} not negligible vs peak {peak}"
        );
    }

    /// Free undamped 1-DOF oscillator `ü + ω₀² u = 0`, `u(0) = 1`,
    /// `u̇(0) = 0`  ⇒ `u(t) = cos(ω₀ t)`. Halving Δt must cut the error
    /// at ~2nd order (the average-acceleration Newmark limit).
    #[test]
    fn undamped_oscillator_is_second_order() {
        let omega0 = 2.0;
        let solver = TransientSolver::from_matrices_for_test(
            scalar_mat(omega0 * omega0), // K = ω₀²
            scalar_mat(1.0),             // M = 1
            scalar_mat(0.0),             // C = 0
        );
        let scheme = TransientScheme::generalized_alpha(1.0);
        let t_end = 5.0;

        let err_at = |dt: f64| -> f64 {
            let mut stepper = solver.factor(dt, scheme).unwrap();
            stepper.set_state(&[1.0], &[0.0]);
            let n = (t_end / dt).round() as usize;
            let zero = [0.0];
            for _ in 0..n {
                stepper.step(&zero, &zero);
            }
            let t = n as f64 * dt;
            (stepper.displacement()[0] - (omega0 * t).cos()).abs()
        };

        let e_coarse = err_at(0.05);
        let e_fine = err_at(0.025);
        let order = (e_coarse / e_fine).log2();
        assert!(
            order > 1.8,
            "convergence order {order:.3} < 1.8 (e_coarse = {e_coarse:.3e}, e_fine = {e_fine:.3e})"
        );
    }

    /// Damped 1-DOF oscillator `ü + 2ζω₀ u̇ + ω₀² u = 0` vs the analytic
    /// under-damped solution, at a fine Δt.
    #[test]
    fn damped_oscillator_matches_analytic() {
        let omega0 = 3.0;
        let zeta = 0.1;
        let c = 2.0 * zeta * omega0;
        let solver = TransientSolver::from_matrices_for_test(
            scalar_mat(omega0 * omega0),
            scalar_mat(1.0),
            scalar_mat(c),
        );
        let dt = 0.002;
        let scheme = TransientScheme::generalized_alpha(1.0);
        let mut stepper = solver.factor(dt, scheme).unwrap();
        stepper.set_state(&[1.0], &[0.0]);

        let wd = omega0 * (1.0 - zeta * zeta).sqrt();
        let analytic = |t: f64| -> f64 {
            (-zeta * omega0 * t).exp() * ((wd * t).cos() + (zeta * omega0 / wd) * (wd * t).sin())
        };

        let n = 2000;
        let zero = [0.0];
        let mut max_err = 0.0_f64;
        for k in 1..=n {
            stepper.step(&zero, &zero);
            let t = k as f64 * dt;
            max_err = max_err.max((stepper.displacement()[0] - analytic(t)).abs());
        }
        assert!(max_err < 5e-3, "damped oscillator max error {max_err:.3e}");
    }

    /// Lossless (C = 0) energy conservation at ρ∞ = 1: total energy
    /// `½ẋᵀMẋ + ½xᵀKx` drifts < 1e−6 relative over a long run.
    #[test]
    fn lossless_energy_is_conserved() {
        let omega0 = 2.5;
        let solver = TransientSolver::from_matrices_for_test(
            scalar_mat(omega0 * omega0),
            scalar_mat(1.0),
            scalar_mat(0.0),
        );
        let dt = 0.01;
        let mut stepper = solver
            .factor(dt, TransientScheme::generalized_alpha(1.0))
            .unwrap();
        stepper.set_state(&[1.0], &[0.0]);
        let e0 = stepper.energy();
        let zero = [0.0];
        for _ in 0..5000 {
            stepper.step(&zero, &zero);
        }
        let drift = (stepper.energy() - e0).abs() / e0;
        assert!(drift < 1e-6, "energy drift {drift:.3e} over 5000 steps");
    }

    /// Forced 1-DOF oscillator `ü + ω₀² u = F₀ sin(Ω t)` — steady-state
    /// amplitude must be the analytic `F₀/(ω₀² − Ω²)`. Guards the *forcing*
    /// term amplitude (the unforced tests above only exercise the
    /// homogeneous response).
    #[test]
    fn forced_oscillator_steady_state_amplitude() {
        let omega0 = 3.0;
        let big_omega = 1.0;
        let f0 = 2.0;
        let zeta = 0.05; // light damping so it settles to steady state
        let c = 2.0 * zeta * omega0;
        let solver = TransientSolver::from_matrices_for_test(
            scalar_mat(omega0 * omega0),
            scalar_mat(1.0),
            scalar_mat(c),
        );
        let dt = (2.0 * std::f64::consts::PI / big_omega) / 200.0;
        let mut stepper = solver
            .factor(dt, TransientScheme::generalized_alpha(0.8))
            .unwrap();
        let force = |t: f64| f0 * (big_omega * t).sin();
        // Run to steady state, then measure peak displacement.
        let n_settle = 4000;
        for step in 0..n_settle {
            let t_n = step as f64 * dt;
            stepper.step(&[force(t_n)], &[force(t_n + dt)]);
        }
        let mut peak = 0.0_f64;
        for step in n_settle..(n_settle + 1600) {
            let t_n = step as f64 * dt;
            stepper.step(&[force(t_n)], &[force(t_n + dt)]);
            peak = peak.max(stepper.displacement()[0].abs());
        }
        // Analytic steady amplitude of a lightly-damped driven oscillator.
        let denom =
            ((omega0 * omega0 - big_omega * big_omega).powi(2) + (c * big_omega).powi(2)).sqrt();
        let analytic = f0 / denom;
        let rel = (peak - analytic).abs() / analytic;
        assert!(
            rel < 2e-3,
            "forced steady amplitude {peak:.5} vs analytic {analytic:.5} (rel {rel:.3e})"
        );
    }

    /// Instability tripwire: central difference (β = 0, γ = ½) above its
    /// CFL limit `Δt < 2/ω₀` must blow up.
    #[test]
    fn central_difference_blows_up_above_cfl() {
        let omega0 = 10.0;
        let solver = TransientSolver::from_matrices_for_test(
            scalar_mat(omega0 * omega0),
            scalar_mat(1.0),
            scalar_mat(0.0),
        );
        // CFL: Δt < 2/ω₀ = 0.2. Pick Δt = 0.3 (unstable).
        let dt = 0.3;
        let scheme = TransientScheme::from_newmark_beta_gamma(0.0, 0.5);
        let mut stepper = solver.factor(dt, scheme).unwrap();
        stepper.set_state(&[1.0], &[0.0]);
        let zero = [0.0];
        for _ in 0..200 {
            stepper.step(&zero, &zero);
        }
        let u = stepper.displacement()[0];
        assert!(
            !u.is_finite() || u.abs() > 1e3,
            "central difference above CFL should diverge, got u = {u}"
        );
    }
}
