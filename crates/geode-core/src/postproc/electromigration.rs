//! Electromigration (EM) lifetime models as pure consumers of a current
//! density `j` and a temperature `T` (issue #1558, piece 3 of #1555).
//!
//! Three independent models, all taking plain scalars so they work with a
//! uniform `T` and need no mesh (a caller can feed `|J|` from
//! [`crate::assembly::conduction::ConductionSolution`]):
//!
//! * [`black_mttf`] — Black's equation `MTTF = A |j|⁻ⁿ exp(Eₐ/kT)`
//!   (J. R. Black, "Electromigration — a brief survey and some recent
//!   results", *IEEE Trans. Electron Devices* ED-16, 338 (1969)).
//! * [`blech_critical_jl`] / [`blech_is_immortal`] — the Blech short-line
//!   immortality condition (I. A. Blech, "Electromigration in thin aluminum
//!   films on titanium nitride", *J. Appl. Phys.* 47, 1203 (1976); Blech &
//!   Herring, *Appl. Phys. Lett.* 29, 131 (1976)).
//! * [`korhonen_evolve`] — the 1-D stress-evolution equation of Korhonen,
//!   Børgesen, Tu & Li, "Stress evolution due to electromigration in
//!   confined metal lines", *J. Appl. Phys.* 73, 3790 (1993).
//!
//! No material constants are defaulted: every constant is a caller input.
//! Only the CODATA constants `k_B` and `e` live here.
//!
//! # Korhonen model and sign convention
//!
//! For a line `0 ≤ x ≤ L` with blocking (zero-atomic-flux) ends and current
//! density `j` along `+x`, with hydrostatic stress `σ` (tension positive):
//!
//! ```text
//! ∂σ/∂t = ∂/∂x [ κ ( ∂σ/∂x + G ) ],   G = Z* e ρ j / Ω,   κ = D B Ω / (k T)
//! ```
//!
//! with blocking BC `∂σ/∂x + G = 0` at both ends. Positive `Z*` and `j`
//! drive atoms along `+x`, so the `x = 0` end is depleted (tensile) and the
//! `x = L` end accumulates (compressive). The atom-conserving steady state is
//!
//! ```text
//! σ(x) = G (L/2 − x),   σ(0) − σ(L) = G L = Z* e ρ j L / Ω.
//! ```
//!
//! # `σ_crit` convention
//!
//! `σ_crit` is the critical **peak tensile stress** (relative to the
//! stress-free initial state) at the depleted end, so the end stress is
//! `|G| L / 2` and the Blech product is
//!
//! ```text
//! (jL)_crit = 2 Ω σ_crit / (|Z*| e ρ).
//! ```
//!
//! If a source instead defines the critical stress *difference* across the
//! line, pass `σ_crit/2` — the factor 2 is a convention, not physics.

use std::fmt;

/// Boltzmann constant in eV/K (CODATA 2018, exact).
pub const K_B_EV: f64 = 8.617_333_262e-5;
/// Boltzmann constant in J/K (CODATA 2018, exact).
pub const K_B_J: f64 = 1.380_649e-23;
/// Elementary charge in C (CODATA 2018, exact).
pub const E_CHARGE: f64 = 1.602_176_634e-19;

/// Invalid-input errors for the EM models.
#[derive(Debug, Clone, PartialEq)]
pub enum EmError {
    /// A parameter that must be finite and strictly positive was not.
    NonPositive(&'static str),
    /// A parameter that must be finite was not.
    NonFinite(&'static str),
    /// A parameter that must be finite and non-negative was negative.
    Negative(&'static str),
    /// Discretisation counts must be at least 2 cells and 1 step.
    BadDiscretisation,
}

impl fmt::Display for EmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositive(n) => write!(f, "electromigration: `{n}` must be finite and > 0"),
            Self::NonFinite(n) => write!(f, "electromigration: `{n}` must be finite"),
            Self::Negative(n) => write!(f, "electromigration: `{n}` must be finite and >= 0"),
            Self::BadDiscretisation => {
                write!(f, "electromigration: need n_cells >= 2 and n_steps >= 1")
            }
        }
    }
}

impl std::error::Error for EmError {}

fn pos(name: &'static str, v: f64) -> Result<(), EmError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(EmError::NonPositive(name))
    }
}

/// Black's equation `MTTF = A |j|⁻ⁿ exp(Eₐ / k_B T)` (Black 1969).
///
/// `a` is the prefactor (units fold `j⁻ⁿ`), `n` the current exponent,
/// `ea_ev` the activation energy in eV, `j` the current density magnitude
/// (sign ignored) and `t_kelvin` the absolute temperature.
///
/// `j = 0` returns the documented limit `+∞` (no drive, no failure).
/// `a ≤ 0`, `T ≤ 0` or non-finite inputs are errors.
pub fn black_mttf(a: f64, n: f64, ea_ev: f64, j: f64, t_kelvin: f64) -> Result<f64, EmError> {
    pos("A", a)?;
    pos("T", t_kelvin)?;
    if !n.is_finite() {
        return Err(EmError::NonFinite("n"));
    }
    if !ea_ev.is_finite() {
        return Err(EmError::NonFinite("Ea"));
    }
    if !j.is_finite() {
        return Err(EmError::NonFinite("j"));
    }
    let j = j.abs();
    if j == 0.0 {
        return Ok(f64::INFINITY);
    }
    Ok(a * j.powf(-n) * (ea_ev / (K_B_EV * t_kelvin)).exp())
}

/// Blech critical product `(jL)_crit = 2 Ω σ_crit / (|Z*| e ρ)` in A/m.
///
/// `omega` atomic volume (m³), `sigma_crit` critical peak tensile stress (Pa,
/// see the module docs for the convention), `z_star` effective charge number,
/// `rho` resistivity (Ω·m). Derived from the Korhonen steady state
/// `σ_end = |Z*| e ρ j L / (2Ω)` (Blech & Herring 1976).
pub fn blech_critical_jl(
    omega: f64,
    sigma_crit: f64,
    z_star: f64,
    rho: f64,
) -> Result<f64, EmError> {
    pos("omega", omega)?;
    pos("sigma_crit", sigma_crit)?;
    pos("rho", rho)?;
    if !z_star.is_finite() || z_star == 0.0 {
        return Err(EmError::NonPositive("|Z*|"));
    }
    Ok(2.0 * omega * sigma_crit / (z_star.abs() * E_CHARGE * rho))
}

/// `true` when a blocked line of length `l` carrying `j` is immortal, i.e.
/// `|j| L < (jL)_crit` (strictly below threshold).
pub fn blech_is_immortal(
    j: f64,
    l: f64,
    omega: f64,
    sigma_crit: f64,
    z_star: f64,
    rho: f64,
) -> Result<bool, EmError> {
    pos("L", l)?;
    if !j.is_finite() {
        return Err(EmError::NonFinite("j"));
    }
    let crit = blech_critical_jl(omega, sigma_crit, z_star, rho)?;
    Ok(j.abs() * l < crit)
}

/// Current-density waveform for the Korhonen solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Waveform {
    /// Constant `j` (A/m², signed along `+x`).
    Dc(f64),
    /// Unipolar rectangular pulse train: `j = peak` for the first
    /// `duty · period` of each period, else `0`.
    Pulsed {
        /// Peak current density (A/m²).
        peak: f64,
        /// Pulse period (s).
        period: f64,
        /// Duty factor in `(0, 1]`.
        duty: f64,
    },
}

impl Waveform {
    /// Time-averaged current density over a full period (the DC value for
    /// [`Waveform::Dc`]): the fast-pulse limit of the drift.
    pub fn average(&self) -> f64 {
        match *self {
            Self::Dc(j) => j,
            Self::Pulsed { peak, duty, .. } => peak * duty,
        }
    }

    /// `∫₀ᵗ j dt'` (exact).
    fn integral(&self, t: f64) -> f64 {
        match *self {
            Self::Dc(j) => j * t,
            Self::Pulsed { peak, period, duty } => {
                let k = (t / period).floor();
                let r = t - k * period;
                let on = duty * period;
                peak * (k * on + r.min(on))
            }
        }
    }

    fn validate(&self) -> Result<(), EmError> {
        match *self {
            Self::Dc(j) if j.is_finite() => Ok(()),
            Self::Dc(_) => Err(EmError::NonFinite("j")),
            Self::Pulsed { peak, period, duty } => {
                if !peak.is_finite() {
                    return Err(EmError::NonFinite("peak"));
                }
                pos("period", period)?;
                if !(duty.is_finite() && duty > 0.0 && duty <= 1.0) {
                    return Err(EmError::NonPositive("duty"));
                }
                Ok(())
            }
        }
    }
}

/// Parameters of a blocked 1-D Korhonen line (all caller-supplied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KorhonenLine {
    /// Line length `L` (m).
    pub length: f64,
    /// Stress diffusivity `κ = D B Ω / (k T)` (m²/s); see [`korhonen_kappa`].
    pub kappa: f64,
    /// Atomic volume `Ω` (m³).
    pub omega: f64,
    /// Effective charge number `Z*` (signed).
    pub z_star: f64,
    /// Resistivity `ρ` (Ω·m).
    pub resistivity: f64,
}

impl KorhonenLine {
    fn validate(&self) -> Result<(), EmError> {
        pos("L", self.length)?;
        pos("kappa", self.kappa)?;
        pos("omega", self.omega)?;
        pos("rho", self.resistivity)?;
        if !self.z_star.is_finite() {
            return Err(EmError::NonFinite("Z*"));
        }
        Ok(())
    }

    /// Drift gradient `G = Z* e ρ j / Ω` (Pa/m) for a current density `j`.
    pub fn drift_gradient(&self, j: f64) -> f64 {
        self.z_star * E_CHARGE * self.resistivity * j / self.omega
    }

    /// Slowest diffusion time constant `τ = L² / (π² κ)` (s).
    pub fn time_constant(&self) -> f64 {
        self.length * self.length / (std::f64::consts::PI.powi(2) * self.kappa)
    }

    /// Steady-state stress at the end `x = 0` for DC `j`:
    /// `G L / 2` (tensile when positive).
    pub fn steady_end_stress(&self, j: f64) -> f64 {
        0.5 * self.drift_gradient(j) * self.length
    }
}

/// `κ = D B Ω / (k_B T)` (m²/s) from the atomic diffusivity `d` (m²/s), the
/// effective bulk modulus `b` (Pa), `omega` (m³) and `T` (K).
pub fn korhonen_kappa(d: f64, b: f64, omega: f64, t_kelvin: f64) -> Result<f64, EmError> {
    pos("D", d)?;
    pos("B", b)?;
    pos("omega", omega)?;
    pos("T", t_kelvin)?;
    Ok(d * b * omega / (K_B_J * t_kelvin))
}

/// Stress profile on cell centres of a Korhonen line.
#[derive(Debug, Clone, PartialEq)]
pub struct KorhonenProfile {
    /// Cell-centre coordinates (m).
    pub x: Vec<f64>,
    /// Hydrostatic stress at the cell centres (Pa, tension positive).
    pub sigma: Vec<f64>,
    /// Stress extrapolated to `x = 0` using the blocking-BC gradient (Pa).
    pub sigma_at_0: f64,
    /// Stress extrapolated to `x = L` (Pa).
    pub sigma_at_l: f64,
}

impl KorhonenProfile {
    /// Largest tensile end stress, `max(σ(0), σ(L))` — the nucleation site.
    pub fn peak_tensile(&self) -> f64 {
        self.sigma_at_0.max(self.sigma_at_l)
    }
}

/// Evolve the blocked Korhonen line from zero stress to `t_end`.
///
/// Cell-centred finite volumes (`n_cells`) with zero flux at both ends and
/// backward-Euler stepping (`n_steps` uniform steps; unconditionally stable,
/// conserves `∫σ dx`). Within each step the drift uses the **exact** average
/// of the waveform over that step, so a pulse train much faster than the
/// step reproduces the time-averaged-drift limit.
pub fn korhonen_evolve(
    line: &KorhonenLine,
    waveform: &Waveform,
    t_end: f64,
    n_cells: usize,
    n_steps: usize,
) -> Result<KorhonenProfile, EmError> {
    line.validate()?;
    waveform.validate()?;
    if !t_end.is_finite() {
        return Err(EmError::NonFinite("t_end"));
    }
    if t_end < 0.0 {
        return Err(EmError::Negative("t_end"));
    }
    if n_cells < 2 || n_steps < 1 {
        return Err(EmError::BadDiscretisation);
    }
    let n = n_cells;
    let h = line.length / n as f64;
    let dt = t_end / n_steps as f64;
    let r = line.kappa * dt / (h * h);
    let mut sigma = vec![0.0_f64; n];

    // Tridiagonal (I + r·L) with Neumann-type ends; scratch for Thomas.
    let mut lower = vec![-r; n];
    let mut diag = vec![1.0 + 2.0 * r; n];
    let mut upper = vec![-r; n];
    diag[0] = 1.0 + r;
    diag[n - 1] = 1.0 + r;
    lower[0] = 0.0;
    upper[n - 1] = 0.0;
    let mut cp = vec![0.0_f64; n];
    let mut dp = vec![0.0_f64; n];
    let mut rhs = vec![0.0_f64; n];

    let mut g = 0.0;
    for step in 0..n_steps {
        if dt > 0.0 {
            let t0 = step as f64 * dt;
            let j_avg = (waveform.integral(t0 + dt) - waveform.integral(t0)) / dt;
            g = line.drift_gradient(j_avg);
        }
        // Source: +κΔt G/h on the first cell, −κΔt G/h on the last cell
        // (interior drift fluxes cancel; boundary fluxes carry the BC).
        let s = line.kappa * dt * g / h;
        rhs.copy_from_slice(&sigma);
        rhs[0] += s;
        rhs[n - 1] -= s;

        cp[0] = upper[0] / diag[0];
        dp[0] = rhs[0] / diag[0];
        for i in 1..n {
            let m = diag[i] - lower[i] * cp[i - 1];
            cp[i] = upper[i] / m;
            dp[i] = (rhs[i] - lower[i] * dp[i - 1]) / m;
        }
        sigma[n - 1] = dp[n - 1];
        for i in (0..n - 1).rev() {
            sigma[i] = dp[i] - cp[i] * sigma[i + 1];
        }
    }

    // Blocking BC: ∂σ/∂x = −G at both ends. G for the extrapolation is the
    // final-step drift (exact for DC and for the steady state).
    let sigma_at_0 = sigma[0] + g * 0.5 * h;
    let sigma_at_l = sigma[n - 1] - g * 0.5 * h;
    let x = (0..n).map(|i| (i as f64 + 0.5) * h).collect();
    Ok(KorhonenProfile {
        x,
        sigma,
        sigma_at_0,
        sigma_at_l,
    })
}
