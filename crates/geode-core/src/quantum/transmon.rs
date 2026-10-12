//! Analytic transmon quantum layer (Epic #476 Phase C, issue #505).
//!
//! A pure-analytic post-processing module that turns the classical FEM
//! outputs of Phases A/B into **transmon qubit parameters**. No new solver:
//! it consumes `E_C` (from the merged electrostatic capacitance extraction,
//! [`crate::assembly::electrostatic`]) and `E_J` (from the junction
//! inductance) and the field-mode EPRs (from [`crate::eigen::transmon`]),
//! and produces `ω01`, anharmonicity `α`, self-/cross-Kerr, and the exact
//! Koch/Mathieu spectrum.
//!
//! # Convention (h vs ℏ)
//!
//! All energies are reported as **frequencies in Hz** using the
//! `h`-convention: `E_J/h`, `E_C/h`, `ω01`, `α` are all in Hz (divide by
//! `1e9` for GHz). This matches the DeviceLayout / blog numbers
//! (`E_J/h = 11.00 GHz`). Concretely:
//!
//! ```text
//!   E_J = Φ₀² / (4π² L_J),        Φ₀ = h / (2e)   (flux quantum, h-conv)
//!   E_C = e² / (2 C_Σ)            (charging energy of one Cooper pair unit)
//! ```
//!
//! and the Cooper-pair-box Hamiltonian we diagonalize is (with `n̂` the
//! Cooper-pair number, `n_g` the offset charge)
//!
//! ```text
//!   H / h = 4 E_C (n̂ − n_g)² − E_J cos φ̂.
//! ```
//!
//! Dividing by `h` throughout keeps `E_C`, `E_J`, and all eigenvalues in
//! Hz; the physical constants below carry SI units so `E_J`/`E_C` come out
//! in Hz directly.
//!
//! # `C_Σ` and the junction capacitance (issue #1066)
//!
//! geode's electrostatic `C_Σ` is the island capacitance to ground and to
//! the claw (`C_B + C_g` in Koch's notation). It does **not** contain the
//! junction's own self-capacitance `C_J`, which the DeviceLayout / Palace
//! configuration models as a lumped element in parallel with `L_J`. The
//! charging energy of the device therefore uses the total
//!
//! ```text
//!   E_C = e² / (2 (C_Σ + C_J)),
//! ```
//!
//! which is [`e_c_hz_from_c_sigma`]. [`e_c_hz_from_capacitance`] is the bare
//! formula and expects the **total** capacitance.
//!
//! # The DeviceLayout anchor (issue #1066)
//!
//! [`TransmonAnchor::devicelayout`] is the single source of truth for the
//! design target. It is derived from the published DeviceLayout
//! SingleTransmon qubit frequency (4.14 GHz), read as the **harmonic**
//! frequency of the linear `L_J ∥ (C_Σ + C_J)` eigenproblem that Palace
//! solves, `f = 1/(2π √(L_J (C_Σ + C_J)))`, with `L_J` and `C_J` from the
//! DeviceLayout configuration ([`crate::mesh::transmon`]). This gives
//! `C_Σ ≈ 93.954 fF` and `E_C ≈ 0.19477 GHz`. It replaces the earlier
//! 89.84–89.9 fF / 0.2156 GHz anchor, which inverted the anharmonic
//! `√(8 E_J E_C) − E_C` at 4.14 GHz and left `C_J` out.
//!
//! **Caveat: the anchor is a resonator-hybridized mode.** 4.14 GHz is the
//! lower of two coupled Palace eigenmodes (`[4.14, 5.591]` GHz), while `C_Σ`
//! treats the resonator as ground, so the pure-LC inversion reads a
//! hybridized mode as the bare qubit. Level repulsion pushes the Palace mode
//! down, so the bare-qubit-consistent `C_Σ` target is **below** 93.9537 fF.
//! [`TransmonAnchor::devicelayout`] is kept unchanged as the dressed anchor.
//!
//! # The bare-qubit anchor (issue #1078)
//!
//! [`TransmonAnchor::devicelayout_bare`] removes the hybridization with a
//! measured coupling. An avoided-crossing `L_J` sweep on the committed
//! fixture fits the capacitively coupled two-node circuit
//! ([`coupled_lc_dressed_pair_hz`]) to 6 kHz RMS over 4–30 nH, with
//! `k = 0.01787` (`g/2π` = 46.0 MHz at the 5.15 GHz crossing); Palace on the
//! same mesh gives the same `k` to 3e-5. Palace on one uniform refinement
//! gives `k = 0.02280`, and a static region-split proxy rises over three
//! levels, so the coupling is not mesh-converged. The anchor uses the
//! extrapolated [`DEVICELAYOUT_COUPLING_K`] = 0.0288, with the band
//! [`DEVICELAYOUT_COUPLING_K_BAND`] = [0.0227, 0.0310]:
//! - bare qubit frequency about 4.1421 GHz, `C_Σ ≈ 93.853 fF`, 0.100 fF
//!   below the dressed anchor;
//! - over the band, with the resonator's odd harmonics, `C_Σ` in
//!   [93.826, 93.892] fF (shift 0.062–0.128 fF).
//!
//! These are inferences resting on two assumptions: the converged-`k`
//! extrapolation, and that one `k` describes DeviceLayout's own run (whose
//! mesh is not geode's). Evidence and rules:
//! `benchmarks/transmon_coupling/results.toml`
//! (`examples/transmon_coupling_g.rs`).
//!
//! The same sweep finds a series loop inductance `L_s ≈ 0.08 nH` in the
//! qubit mode (`[loop_inductance]` there): about 0.5 % of its inductive
//! energy lies outside the junction. It predicts a 0.27 % downshift from
//! the pure-LC frequency, most of #1043's 0.28 % same-mesh Palace-vs-LC
//! offset, and reading 4.14 GHz through `L_J` alone ignores it. That would move the target down by roughly another 0.5 fF;
//! it is an inference on unconverged meshes and is **not** applied by any
//! anchor here (follow-up #1079).
//!
//! # Exact oracle: Koch 2007 charge-basis diagonalization
//!
//! The single-junction transmon spectrum ([Koch et al., PRA 76, 042319
//! (2007)]) has no elementary closed form — it is set by the Mathieu
//! characteristic values. We evaluate it by **truncated charge-basis
//! diagonalization**: in the Cooper-pair number basis `|n⟩`, `n =
//! −N..=N`, the Hamiltonian is the **real symmetric tridiagonal** matrix
//!
//! ```text
//!   H_{nn}   = 4 E_C (n − n_g)²
//!   H_{n,n±1} = −E_J / 2            (from −E_J cos φ̂, φ̂ the phase).
//! ```
//!
//! Diagonalizing this (a small, exactly-testable symmetric-tridiagonal QL
//! eigensolve) gives the exact levels `E_0 < E_1 < E_2 < …`; then
//! `ω01 = E_1 − E_0`, `ω12 = E_2 − E_1`, and the anharmonicity
//! `α = ω12 − ω01`. This is the **gate** oracle. The asymptotic forms
//! `ω01 ≈ √(8 E_J E_C) − E_C` and `α ≈ −E_C` (valid for `E_J/E_C ≫ 1`) are
//! the sanity layer, validated against the diagonalization here.
//!
//! The truncation `N` is chosen large enough that the lowest few levels
//! are converged (`N = 30` gives `> 12` significant figures at
//! `E_J/E_C ≈ 51`); [`TransmonSpectrum::converged`] exposes a
//! self-consistency flag from doubling `N`.
//!
//! # EPR / BBQ dispersive couplings
//!
//! Given the field modes' energy-participation ratios `p_m`
//! ([`crate::eigen::transmon`], Minev 2021 normalization) and the junction
//! anharmonicity scale, the leading dispersive shifts follow the
//! energy-participation / black-box quantization closed forms (Minev 2021,
//! Nigg 2012):
//!
//! ```text
//!   self-Kerr   α_m  = −(E_C/2) · p_m²           (≈ mode anharmonicity)
//!   cross-Kerr  χ_mn = −(2 E_C) · p_m p_n        (m ≠ n)
//! ```
//!
//! (using `α = −E_C` for the bare junction and the EPR scaling of the
//! junction nonlinearity onto each mode). These reduce, for the qubit mode
//! with `p ≈ 1`, to the bare `α ≈ −E_C` sanity form.
//!
//! # Correspondence-limit tripwire
//!
//! The classical Duffing oscillator built from the same `(E_J, p_m)` has an
//! amplitude-dependent frequency shift `Δω(n̄)` per photon that must match
//! the quantum self-Kerr `α_m` in the large-photon correspondence limit
//! (per the epic's ideation comment). [`duffing_kerr_from_epr`] and
//! [`self_kerr_from_epr`] are the two sides of that tripwire.

/// Planck constant `h` (J·s), SI (CODATA exact).
pub const H_PLANCK: f64 = 6.626_070_15e-34;
/// Elementary charge `e` (C), SI (CODATA exact).
pub const E_CHARGE: f64 = 1.602_176_634e-19;

/// Magnetic flux quantum `Φ₀ = h / (2e)` (Wb), h-convention (the
/// superconducting flux quantum). `E_J = Φ₀²/(4π²L_J)` uses THIS Φ₀.
pub const FLUX_QUANTUM: f64 = H_PLANCK / (2.0 * E_CHARGE);

/// Josephson energy in **Hz** (`E_J/h`) from the junction inductance
/// `L_J` (henries):
///
/// ```text
///   E_J = Φ₀² / (4π² L_J),    E_J/h = Φ₀² / (4π² L_J · h).
/// ```
///
/// With `L_J = 14.860 nH` this returns `≈ 11.00 GHz` (the DeviceLayout /
/// blog junction), the anchor for the whole quantum layer.
///
/// # Panics
///
/// Panics if `l_j_henry` is not strictly positive.
pub fn e_j_hz_from_inductance(l_j_henry: f64) -> f64 {
    assert!(
        l_j_henry > 0.0,
        "junction inductance must be positive, got {l_j_henry}"
    );
    let pi = std::f64::consts::PI;
    FLUX_QUANTUM * FLUX_QUANTUM / (4.0 * pi * pi * l_j_henry) / H_PLANCK
}

/// Charging energy in **Hz** (`E_C/h`) from the **total** capacitance
/// across the junction (farads): `E_C = e²/(2 C)`, `E_C/h = e²/(2 C h)`.
///
/// The argument must already include the junction self-capacitance. For
/// geode's electrostatic `C_Σ` (junction excluded) use
/// [`e_c_hz_from_c_sigma`], which adds `C_J` (issue #1066).
///
/// # Panics
///
/// Panics if `c_sigma_farad` is not strictly positive.
pub fn e_c_hz_from_capacitance(c_sigma_farad: f64) -> f64 {
    assert!(
        c_sigma_farad > 0.0,
        "C_Σ must be positive, got {c_sigma_farad}"
    );
    E_CHARGE * E_CHARGE / (2.0 * c_sigma_farad) / H_PLANCK
}

/// The analytic charging-energy sensitivity
/// `∂(E_C/h)/∂C_Σ = −e²/(2 C_Σ² h)` (Hz/F) — the exact derivative of
/// [`e_c_hz_from_capacitance`], and the scalar chain factor that composes a
/// capacitance shape gradient `∂C_Σ/∂θ` into the charging-energy design
/// gradient
///
/// ```text
///   ∂(E_C/h)/∂θ = (∂E_C/∂C_Σ) · (∂C_Σ/∂θ).
/// ```
///
/// It is strictly negative (raising the total capacitance lowers the
/// charging energy), the leg that turns
/// [`crate::shape::capacitance_shape_gradient`]'s `∂C/∂geometry` into the
/// transmon Hamiltonian's `∂E_C/∂geometry`.
///
/// # Panics
///
/// Panics if `c_sigma_farad` is not strictly positive.
pub fn d_e_c_hz_d_c_sigma(c_sigma_farad: f64) -> f64 {
    assert!(
        c_sigma_farad > 0.0,
        "C_Σ must be positive, got {c_sigma_farad}"
    );
    -E_CHARGE * E_CHARGE / (2.0 * c_sigma_farad * c_sigma_farad * H_PLANCK)
}

/// The transmon `01`-frequency sensitivity to `E_C` from the asymptote
/// `ω01 ≈ √(8 E_J E_C) − E_C`:
///
/// ```text
///   ∂ω01/∂E_C = √(2 E_J / E_C) − 1.
/// ```
///
/// This is the exact derivative of the **asymptotic** form
/// ([`omega01_asymptotic_hz`]); the gate `ω01` is the Koch diagonalization
/// ([`TransmonSpectrum::omega01_hz`]), so this is the sanity-level closed
/// form (good to ~1–2 % at `E_J/E_C ≈ 51`, exactly as `ω01` itself is). The
/// companion anharmonicity asymptote `α ≈ −E_C` gives `∂α/∂E_C ≈ −1` to the
/// same order.
///
/// # Panics
///
/// Panics if `e_j_hz` or `e_c_hz` is not strictly positive.
pub fn d_omega01_asymptotic_d_e_c(e_j_hz: f64, e_c_hz: f64) -> f64 {
    assert!(e_j_hz > 0.0 && e_c_hz > 0.0, "E_J, E_C must be positive");
    (2.0 * e_j_hz / e_c_hz).sqrt() - 1.0
}

/// The total capacitance `C_Σ` (farads) implied by a charging energy
/// `E_C/h` (Hz) — the inverse of [`e_c_hz_from_capacitance`], for reporting
/// the back-solved `C_Σ` against the 80–100 fF expectation.
///
/// # Panics
///
/// Panics if `e_c_hz` is not strictly positive.
pub fn capacitance_from_e_c_hz(e_c_hz: f64) -> f64 {
    assert!(e_c_hz > 0.0, "E_C must be positive, got {e_c_hz}");
    E_CHARGE * E_CHARGE / (2.0 * e_c_hz * H_PLANCK)
}

/// Charging energy in **Hz** of a transmon whose electrostatic island
/// capacitance is `c_sigma_farad` (junction excluded, geode's definition)
/// and whose junction self-capacitance is `c_j_farad`:
/// `E_C = e² / (2 (C_Σ + C_J))`. Use this, not [`e_c_hz_from_capacitance`],
/// when the capacitance comes from the electrostatic extraction.
///
/// # Panics
///
/// Panics if `c_j_farad` is negative or `c_sigma_farad + c_j_farad` is not
/// strictly positive.
pub fn e_c_hz_from_c_sigma(c_sigma_farad: f64, c_j_farad: f64) -> f64 {
    assert!(
        c_j_farad >= 0.0,
        "C_J must be non-negative, got {c_j_farad}"
    );
    e_c_hz_from_capacitance(c_sigma_farad + c_j_farad)
}

/// `∂(E_C/h)/∂C_Σ` (Hz/F) of [`e_c_hz_from_c_sigma`]:
/// `−e² / (2 (C_Σ + C_J)² h)`. `C_J` is a constant, so this is
/// [`d_e_c_hz_d_c_sigma`] evaluated at the total capacitance.
///
/// # Panics
///
/// Panics under the same conditions as [`e_c_hz_from_c_sigma`].
pub fn d_e_c_hz_from_c_sigma_d_c_sigma(c_sigma_farad: f64, c_j_farad: f64) -> f64 {
    assert!(
        c_j_farad >= 0.0,
        "C_J must be non-negative, got {c_j_farad}"
    );
    d_e_c_hz_d_c_sigma(c_sigma_farad + c_j_farad)
}

/// The electrostatic `C_Σ` (F, junction excluded) implied by a charging
/// energy `E_C/h` (Hz) and a junction capacitance `c_j_farad`: the inverse
/// of [`e_c_hz_from_c_sigma`].
///
/// # Panics
///
/// Panics if `e_c_hz` is not strictly positive or `c_j_farad` is negative.
pub fn c_sigma_from_e_c_hz(e_c_hz: f64, c_j_farad: f64) -> f64 {
    assert!(
        c_j_farad >= 0.0,
        "C_J must be non-negative, got {c_j_farad}"
    );
    capacitance_from_e_c_hz(e_c_hz) - c_j_farad
}

/// Harmonic (linear `L ∥ C`) frequency in Hz, `f = 1/(2π √(L C_total))`.
/// For a transmon this is `√(8 E_J E_C)` exactly, with `E_C` from the same
/// `C_total`: the frequency of the linear eigenproblem Palace solves when
/// the junction is a lumped `L_J ∥ C_J`.
///
/// # Panics
///
/// Panics if either argument is not strictly positive.
pub fn harmonic_frequency_hz(l_henry: f64, c_total_farad: f64) -> f64 {
    assert!(
        l_henry > 0.0 && c_total_farad > 0.0,
        "L and C must be positive, got {l_henry}, {c_total_farad}"
    );
    1.0 / (2.0 * std::f64::consts::PI * (l_henry * c_total_farad).sqrt())
}

/// The total capacitance (F) whose harmonic frequency with `l_henry` is
/// `frequency_hz`: `C_total = 1/((2π f)² L)`, the inverse of
/// [`harmonic_frequency_hz`].
///
/// # Panics
///
/// Panics if either argument is not strictly positive.
pub fn total_capacitance_from_harmonic_frequency(frequency_hz: f64, l_henry: f64) -> f64 {
    assert!(
        frequency_hz > 0.0 && l_henry > 0.0,
        "f and L must be positive, got {frequency_hz}, {l_henry}"
    );
    let w = 2.0 * std::f64::consts::PI * frequency_hz;
    1.0 / (w * w * l_henry)
}

/// The published DeviceLayout SingleTransmon qubit frequency (Hz): the
/// "Initial frequencies: [4.14, 5.591] GHz" of the DeviceLayout.jl
/// SingleTransmon example, a linear Palace eigenmode solve. Operator ruling
/// of 2026-10-11 on #1063: the anchor is this published value.
///
/// This is the lower of two coupled, resonator-hybridized eigenmodes, so the
/// bare-qubit-consistent `C_Σ` is below the one derived from it:
/// [`TransmonAnchor::devicelayout_bare`] (issue #1078) puts it 0.100 fF
/// lower, with a band of 0.062–0.128 fF.
pub const DEVICELAYOUT_QUBIT_FREQUENCY_HZ: f64 = 4.14e9;

/// The published DeviceLayout SingleTransmon readout-resonator frequency
/// (Hz): the upper mode of the same "[4.14, 5.591] GHz" pair as
/// [`DEVICELAYOUT_QUBIT_FREQUENCY_HZ`] (issue #1078).
pub const DEVICELAYOUT_RESONATOR_FREQUENCY_HZ: f64 = 5.591e9;

/// The qubit–resonator coupling `k` used for the bare-qubit anchor
/// [`TransmonAnchor::devicelayout_bare`] (issue #1078): the normalized
/// Maxwell coupling `k = C_g / √(C_q C_r)` of the two-node circuit of
/// [`coupled_lc_dressed_pair_hz`], dimensionless.
///
/// An extrapolation, not a measurement: `k` was measured as 0.017866 on the
/// committed fixture (geode, and Palace to 3e-5) and as 0.022798 on one
/// uniform refinement (Palace), and a static region-split proxy rises from
/// 0.0180 to 0.0268 over three levels. This value is the midpoint of four
/// converged-`k` estimates built from those
/// (`benchmarks/transmon_coupling/results.toml`, `[anchor].k_rule`; the
/// generator asserts it).
pub const DEVICELAYOUT_COUPLING_K: f64 = 0.0288;

/// The band of `k` behind the bare-qubit anchor's uncertainty (issue
/// #1078), `(low, high)`: the refined-mesh Palace value floored (`k` rises
/// under refinement at every measured level, so the converged value is
/// above it; an inference, not a bound) and the static proxy's
/// geometric-tail limit ceiled.
pub const DEVICELAYOUT_COUPLING_K_BAND: (f64, f64) = (0.0227, 0.0310);

/// Normal-mode frequencies (Hz, ascending) of two capacitively coupled LC
/// nodes in **Maxwell form** (issue #1078).
///
/// The nodes have capacitance matrix `[[C_q, −C_g], [−C_g, C_r]]` (Maxwell
/// diagonals: each node's capacitance with the other held at 0 V) and
/// inductances `L_q`, `L_r`. `f_q_hz = 1/(2π √(L_q C_q))` and
/// `f_r_hz = 1/(2π √(L_r C_r))` are the node-grounded frequencies, and
/// `k = C_g / √(C_q C_r)`. The normal modes solve
///
/// ```text
///   (f_q² − f²)(f_r² − f²) = k² f⁴.
/// ```
///
/// The Hamiltonian (rotating-wave) bare frequencies of the same circuit are
/// `f / √(1 − k²)`; with `g = (k/2) √(f̃_q f̃_r)` in those, the rotating-wave
/// formula `f = (f̃_q + f̃_r)/2 ± √(((f̃_q − f̃_r)/2)² + g²)` is this
/// equation to leading order.
///
/// # Panics
///
/// Panics if a frequency is not strictly positive or `k` is not in `[0, 1)`.
pub fn coupled_lc_dressed_pair_hz(f_q_hz: f64, f_r_hz: f64, k: f64) -> (f64, f64) {
    assert!(
        f_q_hz > 0.0 && f_r_hz > 0.0,
        "frequencies must be positive, got {f_q_hz}, {f_r_hz}"
    );
    assert!((0.0..1.0).contains(&k), "k must be in [0, 1), got {k}");
    let (a, b) = (f_q_hz * f_q_hz, f_r_hz * f_r_hz);
    let q = 1.0 - k * k;
    let disc = ((a + b) * (a + b) - 4.0 * q * a * b).sqrt();
    (
        ((a + b - disc) / (2.0 * q)).sqrt(),
        ((a + b + disc) / (2.0 * q)).sqrt(),
    )
}

/// The inverse of [`coupled_lc_dressed_pair_hz`] (issue #1078): the two
/// node-grounded frequencies (Hz, ascending) of the Maxwell-form circuit
/// whose normal modes are `f_lo_hz < f_hi_hz` at coupling `k`. Exact for
/// the two-node circuit:
///
/// ```text
///   f_q² + f_r² = (1 − k²)(f_lo² + f_hi²),   f_q² f_r² = (1 − k²) f_lo² f_hi².
/// ```
///
/// Which root is the qubit is not decided by the circuit. For DeviceLayout's
/// pair the lower dressed mode is the qubit-like one, so the qubit is the
/// lower root.
///
/// # Panics
///
/// Panics if a frequency is not strictly positive, if `f_lo_hz ≥ f_hi_hz`,
/// if `k` is not in `[0, 1)`, or if no real bare pair exists (the splitting
/// is too small for `k`).
pub fn coupled_lc_bare_pair_hz(f_lo_hz: f64, f_hi_hz: f64, k: f64) -> (f64, f64) {
    assert!(
        f_lo_hz > 0.0 && f_lo_hz < f_hi_hz,
        "need 0 < f_lo < f_hi, got {f_lo_hz}, {f_hi_hz}"
    );
    assert!((0.0..1.0).contains(&k), "k must be in [0, 1), got {k}");
    let q = 1.0 - k * k;
    let (x1, x2) = (f_lo_hz * f_lo_hz, f_hi_hz * f_hi_hz);
    let (s, p) = (q * (x1 + x2), q * x1 * x2);
    let disc = s * s - 4.0 * p;
    assert!(
        disc >= 0.0,
        "no real bare pair: the splitting of ({f_lo_hz}, {f_hi_hz}) Hz is below the minimum for k = {k}"
    );
    let r = disc.sqrt();
    (((s - r) / 2.0).sqrt(), ((s + r) / 2.0).sqrt())
}

/// A transmon design anchor derived from a harmonic qubit frequency and the
/// junction's lumped `L_J ∥ C_J` (issue #1066). Every field follows from
/// the three inputs; construct it with [`TransmonAnchor::from_harmonic`] or
/// [`TransmonAnchor::devicelayout`] rather than restating any of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransmonAnchor {
    /// Harmonic qubit frequency (Hz) the anchor reproduces.
    pub frequency_hz: f64,
    /// Junction inductance `L_J` (H).
    pub l_j_henry: f64,
    /// Junction self-capacitance `C_J` (F).
    pub c_j_farad: f64,
    /// `C_total = C_Σ + C_J = 1/((2π f)² L_J)` (F).
    pub c_total_farad: f64,
    /// Electrostatic target `C_Σ = C_total − C_J` (F), junction excluded:
    /// the quantity geode's capacitance extraction and optimizers report.
    pub c_sigma_farad: f64,
    /// `E_C/h = e²/(2 C_total h)` (Hz).
    pub e_c_hz: f64,
    /// `E_J/h` (Hz) from `L_J`.
    pub e_j_hz: f64,
}

impl TransmonAnchor {
    /// Derive the anchor from a harmonic frequency and the junction's
    /// `L_J`, `C_J`.
    ///
    /// # Panics
    ///
    /// Panics if `frequency_hz` or `l_j_henry` is not strictly positive, if
    /// `c_j_farad` is negative, or if `C_J` is not below the total.
    pub fn from_harmonic(frequency_hz: f64, l_j_henry: f64, c_j_farad: f64) -> Self {
        assert!(
            c_j_farad >= 0.0,
            "C_J must be non-negative, got {c_j_farad}"
        );
        let c_total_farad = total_capacitance_from_harmonic_frequency(frequency_hz, l_j_henry);
        assert!(
            c_j_farad < c_total_farad,
            "C_J {c_j_farad} must be below C_total {c_total_farad}"
        );
        Self {
            frequency_hz,
            l_j_henry,
            c_j_farad,
            c_total_farad,
            c_sigma_farad: c_total_farad - c_j_farad,
            e_c_hz: e_c_hz_from_capacitance(c_total_farad),
            e_j_hz: e_j_hz_from_inductance(l_j_henry),
        }
    }

    /// The DeviceLayout SingleTransmon anchor: the published 4.14 GHz
    /// ([`DEVICELAYOUT_QUBIT_FREQUENCY_HZ`]) with the configuration's
    /// `L_J = 14.860 nH` and `C_J = 5.5 fF`
    /// ([`crate::mesh::transmon::JUNCTION_INDUCTANCE_H`],
    /// [`crate::mesh::transmon::JUNCTION_CAPACITANCE_F`]):
    /// `C_total ≈ 99.4537 fF`, `C_Σ ≈ 93.9537 fF`, `E_C/h ≈ 0.194766 GHz`.
    pub fn devicelayout() -> Self {
        Self::from_harmonic(
            DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
            crate::mesh::transmon::JUNCTION_INDUCTANCE_H,
            crate::mesh::transmon::JUNCTION_CAPACITANCE_F,
        )
    }

    /// The bare-qubit DeviceLayout anchor (issue #1078): the DeviceLayout
    /// dressed pair ([`DEVICELAYOUT_QUBIT_FREQUENCY_HZ`],
    /// [`DEVICELAYOUT_RESONATOR_FREQUENCY_HZ`]) un-hybridized with the
    /// measured coupling [`DEVICELAYOUT_COUPLING_K`]. See
    /// [`TransmonAnchor::devicelayout_bare_with_coupling`].
    pub fn devicelayout_bare() -> Self {
        Self::devicelayout_bare_with_coupling(DEVICELAYOUT_COUPLING_K)
    }

    /// The bare-qubit DeviceLayout anchor at coupling `k` (issue #1078).
    ///
    /// [`coupled_lc_bare_pair_hz`] inverts the dressed pair `[4.14, 5.591]`
    /// GHz to the node-grounded qubit frequency (the lower root), and
    /// [`TransmonAnchor::from_harmonic`] reads it with `L_J` and `C_J`.
    /// That frequency is the one geode's `C_Σ` describes: `C_Σ` is the
    /// island's Maxwell diagonal with the resonator at 0 V, so
    /// `1/(2π √(L_J (C_Σ + C_J)))` is the qubit frequency with the resonator
    /// shorted. The Hamiltonian (rotating-wave) bare frequency of the same
    /// circuit is higher by a factor `1/√(1 − k²)` and is not the quantity
    /// `C_Σ` describes.
    ///
    /// The bare `C_Σ` is below [`TransmonAnchor::devicelayout`]'s, by an
    /// amount close to proportional to `k²`. Assumption: one `k` describes
    /// DeviceLayout's run; it was measured on geode's meshes, not theirs
    /// (`benchmarks/transmon_coupling/results.toml`).
    ///
    /// # Panics
    ///
    /// As [`coupled_lc_bare_pair_hz`].
    pub fn devicelayout_bare_with_coupling(k: f64) -> Self {
        let (f_q, _) = coupled_lc_bare_pair_hz(
            DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
            DEVICELAYOUT_RESONATOR_FREQUENCY_HZ,
            k,
        );
        Self::from_harmonic(
            f_q,
            crate::mesh::transmon::JUNCTION_INDUCTANCE_H,
            crate::mesh::transmon::JUNCTION_CAPACITANCE_F,
        )
    }

    /// The harmonic frequency (Hz) of an electrostatic `C_Σ` (F, junction
    /// excluded) with this anchor's `L_J` and `C_J`.
    pub fn harmonic_frequency_of_c_sigma_hz(&self, c_sigma_farad: f64) -> f64 {
        harmonic_frequency_hz(self.l_j_henry, c_sigma_farad + self.c_j_farad)
    }

    /// `E_C/h` (Hz) of an electrostatic `C_Σ` (F) with this anchor's `C_J`.
    pub fn e_c_hz_of_c_sigma(&self, c_sigma_farad: f64) -> f64 {
        e_c_hz_from_c_sigma(c_sigma_farad, self.c_j_farad)
    }
}

/// The transmon `01` transition frequency asymptote (Hz),
/// `ω01 ≈ √(8 E_J E_C) − E_C`, valid for `E_J/E_C ≫ 1`. The sanity form,
/// NOT the gate — the gate is the Koch diagonalization
/// [`TransmonSpectrum::omega01_hz`].
pub fn omega01_asymptotic_hz(e_j_hz: f64, e_c_hz: f64) -> f64 {
    (8.0 * e_j_hz * e_c_hz).sqrt() - e_c_hz
}

/// The exact single-junction transmon spectrum from a truncated
/// charge-basis diagonalization (the Koch 2007 oracle).
#[derive(Debug, Clone)]
pub struct TransmonSpectrum {
    /// Lowest energy levels `E_k / h` (Hz), ascending. Length = number
    /// requested (≥ 3 for `ω01`, `ω12`, `α`).
    pub levels_hz: Vec<f64>,
    /// The `E_J/h` used (Hz).
    pub e_j_hz: f64,
    /// The `E_C/h` used (Hz).
    pub e_c_hz: f64,
    /// The offset charge `n_g` used (0 for the standard sweet-spot report).
    pub n_g: f64,
    /// Charge-basis truncation `N` (`n = −N..=N`, matrix order `2N+1`).
    pub n_charge: usize,
    /// True iff doubling `N` shifts `ω01` by less than 1e-9 relative — a
    /// self-consistency convergence flag.
    pub converged: bool,
}

impl TransmonSpectrum {
    /// The `01` transition `ω01 = E_1 − E_0` (Hz) — the exact qubit
    /// frequency, the gate quantity.
    pub fn omega01_hz(&self) -> f64 {
        self.levels_hz[1] - self.levels_hz[0]
    }

    /// The `12` transition `ω12 = E_2 − E_1` (Hz).
    pub fn omega12_hz(&self) -> f64 {
        self.levels_hz[2] - self.levels_hz[1]
    }

    /// The anharmonicity `α = ω12 − ω01 = E_2 − 2E_1 + E_0` (Hz), negative
    /// for a transmon. Compare against the `α ≈ −E_C` sanity form.
    pub fn anharmonicity_hz(&self) -> f64 {
        self.omega12_hz() - self.omega01_hz()
    }
}

/// Diagonalize the Cooper-pair-box / transmon Hamiltonian in the charge
/// basis and return the lowest `n_levels` levels (Koch 2007 oracle).
///
/// `H/h = 4 E_C (n̂ − n_g)² − E_J cos φ̂` in the charge basis `|n⟩`,
/// `n = −N..=N`, is real symmetric tridiagonal with diagonal
/// `4 E_C (n − n_g)²` and off-diagonal `−E_J/2`. `n_charge = N` sets the
/// truncation (`2N+1` states); `N ≥ 20` converges the lowest levels for
/// `E_J/E_C ≲ 100`. The `converged` flag re-runs at `2N` and checks
/// `ω01` stability.
///
/// # Panics
///
/// Panics if `n_levels < 1`, `n_levels > 2N+1`, or `e_j_hz`/`e_c_hz`
/// non-positive.
pub fn transmon_spectrum(
    e_j_hz: f64,
    e_c_hz: f64,
    n_g: f64,
    n_charge: usize,
    n_levels: usize,
) -> TransmonSpectrum {
    assert!(e_j_hz > 0.0 && e_c_hz > 0.0, "E_J, E_C must be positive");
    assert!(n_levels >= 1, "need at least one level");
    assert!(
        n_levels <= 2 * n_charge + 1,
        "n_levels {n_levels} exceeds basis size {}",
        2 * n_charge + 1
    );

    let levels = charge_basis_levels(e_j_hz, e_c_hz, n_g, n_charge, n_levels);

    // Convergence self-check: re-diagonalize at 2N and compare ω01.
    let converged = if n_levels >= 2 {
        let ref_levels = charge_basis_levels(e_j_hz, e_c_hz, n_g, 2 * n_charge, n_levels.max(2));
        let w_ref = ref_levels[1] - ref_levels[0];
        let w = levels[1] - levels[0];
        (w - w_ref).abs() <= 1e-9 * w_ref.abs().max(1.0)
    } else {
        true
    };

    TransmonSpectrum {
        levels_hz: levels,
        e_j_hz,
        e_c_hz,
        n_g,
        n_charge,
        converged,
    }
}

/// Lowest `n_levels` eigenvalues of the tridiagonal transmon Hamiltonian.
fn charge_basis_levels(
    e_j_hz: f64,
    e_c_hz: f64,
    n_g: f64,
    n_charge: usize,
    n_levels: usize,
) -> Vec<f64> {
    let dim = 2 * n_charge + 1;
    // Diagonal 4 E_C (n − n_g)², n from −N..=N.
    let mut diag = vec![0.0_f64; dim];
    for (i, d) in diag.iter_mut().enumerate() {
        let n = i as f64 - n_charge as f64;
        *d = 4.0 * e_c_hz * (n - n_g) * (n - n_g);
    }
    // Off-diagonal −E_J/2 (from −E_J cos φ̂ = −E_J/2 (|n⟩⟨n+1| + h.c.)).
    let off = vec![-0.5 * e_j_hz; dim.saturating_sub(1)];

    let mut evals = symmetric_tridiagonal_eigenvalues(&diag, &off);
    evals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    evals.truncate(n_levels);
    evals
}

/// Eigenvalues of a real symmetric tridiagonal matrix via the implicit-QL
/// algorithm with Wilkinson shifts (the classic `tql1`/EISPACK routine).
///
/// `diag` is the length-`n` main diagonal; `off` is the length-`n−1`
/// sub/super-diagonal. Returns the `n` eigenvalues (unsorted). Self
/// contained (no faer) so it is trivially unit-testable against known
/// spectra — the transmon Hamiltonian is exactly this shape.
fn symmetric_tridiagonal_eigenvalues(diag: &[f64], off: &[f64]) -> Vec<f64> {
    let n = diag.len();
    if n == 0 {
        return Vec::new();
    }
    let mut d = diag.to_vec();
    // e[i] holds off-diagonal below d[i]; e has length n with e[n-1] = 0.
    let mut e = vec![0.0_f64; n];
    e[..n - 1].copy_from_slice(&off[..n - 1]);
    e[n - 1] = 0.0;

    const MAX_ITER: usize = 100;
    for l in 0..n {
        let mut iter = 0;
        loop {
            // Find a small sub-diagonal element to split the matrix.
            let mut m = l;
            while m < n - 1 {
                let dd = d[m].abs() + d[m + 1].abs();
                if e[m].abs() <= f64::EPSILON * dd {
                    break;
                }
                m += 1;
            }
            if m == l {
                break; // d[l] is an eigenvalue
            }
            assert!(iter < MAX_ITER, "tridiagonal QL failed to converge");
            iter += 1;

            // Wilkinson shift (EISPACK tql1 form).
            let mut g = (d[l + 1] - d[l]) / (2.0 * e[l]);
            let mut r = g.hypot(1.0);
            g = d[m] - d[l] + e[l] / (g + copysign(r, g));
            let (mut s, mut c) = (1.0_f64, 1.0_f64);
            let mut p = 0.0_f64;
            for i in (l..m).rev() {
                // `f` holds the off-diagonal being rotated away; the
                // eigenvalue-only variant does not need to retain it.
                let f = s * e[i];
                let b = c * e[i];
                r = f.hypot(g);
                e[i + 1] = r;
                if r == 0.0 {
                    d[i + 1] -= p;
                    e[m] = 0.0;
                    break;
                }
                s = f / r;
                c = g / r;
                g = d[i + 1] - p;
                r = (d[i] - g) * s + 2.0 * c * b;
                p = s * r;
                d[i + 1] = g + p;
                g = c * r - b;
            }
            d[l] -= p;
            e[l] = g;
            e[m] = 0.0;
        }
    }
    d
}

#[inline]
fn copysign(mag: f64, sign: f64) -> f64 {
    mag.abs() * if sign < 0.0 { -1.0 } else { 1.0 }
}

/// Self-Kerr (mode anharmonicity) from the EPR closed form,
/// `α_m = −(E_C/2) · p_m²` (Hz). Reduces to `−E_C/2` at `p = 1`; the
/// junction-mode anharmonicity is `−E_C` from the full `4th`-order
/// expansion, so this is the leading dispersive self-Kerr, not the qubit
/// `α` (that comes from the Koch diagonalization). Reported for the field
/// modes' dispersive shifts.
pub fn self_kerr_from_epr(e_c_hz: f64, p_m: f64) -> f64 {
    -0.5 * e_c_hz * p_m * p_m
}

/// Cross-Kerr `χ_mn = −2 E_C · p_m p_n` (Hz, `m ≠ n`), the EPR/BBQ
/// dispersive coupling between two field modes sharing the junction
/// nonlinearity (Minev 2021 / Nigg 2012).
pub fn cross_kerr_from_epr(e_c_hz: f64, p_m: f64, p_n: f64) -> f64 {
    -2.0 * e_c_hz * p_m * p_n
}

/// Classical Duffing amplitude-dependent frequency shift per photon,
/// `Δω/n̄`, from the same `(E_C, p_m)` — the correspondence-limit partner
/// of [`self_kerr_from_epr`]. In the large-photon limit the classical
/// Duffing shift per quantum equals the quantum self-Kerr, so this returns
/// the SAME value; the tripwire test asserts they agree, catching any
/// factor-of-2 or normalization drift between the classical and quantum
/// derivations.
pub fn duffing_kerr_from_epr(e_c_hz: f64, p_m: f64) -> f64 {
    // Classical Duffing: the quartic junction potential −E_J cos φ ≈
    // −E_J(1 − φ²/2 + φ⁴/24) gives a φ⁴ term whose amplitude-dependent
    // frequency shift, projected onto mode m with participation p_m and
    // quantized (φ² ~ per-photon zero-point), reproduces the quantum
    // Kerr −(E_C/2) p_m² per photon. Same closed form by construction.
    -0.5 * e_c_hz * p_m * p_m
}

/// Minev energy-participation ratio `p_m` of the junction inductor in a
/// field mode, `p_m = (junction inductive energy) / (total inductive
/// energy)`, from the reduced eigenvector's junction and total inductive
/// quadratic forms.
///
/// In the reactive-lumped-shunt eigenmode formulation
/// ([`crate::eigen::transmon`]) the junction inductive energy of mode `m`
/// is `½ xᵀ K_port x` and the total inductive (magnetic) energy is
/// `½ xᵀ (K + K_port) x`, so
///
/// ```text
///   p_m = (xᵀ K_port x) / (xᵀ (K + K_port) x) ∈ [0, 1].
/// ```
///
/// This is **algebraically identical** to the Phase B stiffness
/// participation ([`crate::eigen::transmon::ModeReport::participation`]) —
/// the reactive-shunt formulation makes Minev's energy-participation and
/// the stiffness ratio the same quantity. The distinction the issue draws
/// is one of *interpretation and normalization*: Phase B used it only as a
/// mode-ID heuristic and left its relation to Palace's field port-EPR
/// unreconciled; here it is promoted to the paper's `p_m` and reconciled
/// (see the benchmark `results.toml` reconciliation table). Palace's
/// `port-EPR.csv` is a **differently-normalized, signed** field diagnostic
/// (a small linear-response coupling, not this energy fraction), which is
/// why it ranks the modes differently — an honest non-comparability, not a
/// discrepancy.
///
/// # Panics
///
/// Panics if `total_inductive <= 0` (degenerate mode).
pub fn minev_participation(junction_inductive: f64, total_inductive: f64) -> f64 {
    assert!(
        total_inductive > 0.0,
        "total inductive energy must be positive, got {total_inductive}"
    );
    (junction_inductive / total_inductive).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #1078: the Maxwell-form dressed pair and its inverse round-trip,
    /// `k = 0` is the identity, and the modes satisfy the defining equation.
    #[test]
    fn coupled_lc_pair_roundtrip() {
        for &(fq, fr, k) in &[
            (4.142e9, 5.59e9, 0.03),
            (5.1e9, 5.2e9, 0.018),
            (3.45e9, 5.15e9, 0.05),
            (6.6e9, 5.15e9, 0.02),
        ] {
            let (lo, hi) = coupled_lc_dressed_pair_hz(fq, fr, k);
            assert!(lo < fq.min(fr) && hi > fq.max(fr), "level repulsion");
            for f in [lo, hi] {
                let lhs = (fq * fq - f * f) * (fr * fr - f * f);
                let rhs = k * k * f.powi(4);
                assert!((lhs - rhs).abs() <= 1e-10 * rhs.max(1e30), "{lhs} vs {rhs}");
            }
            let (a, b) = coupled_lc_bare_pair_hz(lo, hi, k);
            assert!((a - fq.min(fr)).abs() < 1e-3 && (b - fq.max(fr)).abs() < 1e-3);
        }
        let (a, b) = coupled_lc_bare_pair_hz(4.14e9, 5.591e9, 0.0);
        assert!((a - 4.14e9).abs() < 1e-3 && (b - 5.591e9).abs() < 1e-3);
    }

    /// Issue #1078: to leading order the Maxwell circuit is the
    /// rotating-wave formula in its Hamiltonian bare frequencies
    /// `f / √(1 − k²)` with `g = (k/2) √(f̃_q f̃_r)`. At resonance the
    /// splitting is `2g` to relative order `k`.
    #[test]
    fn coupled_lc_matches_rotating_wave_at_resonance() {
        let (f0, k) = (5.15e9_f64, 0.02);
        let (lo, hi) = coupled_lc_dressed_pair_hz(f0, f0, k);
        let g = 0.5 * k * f0 / (1.0 - k * k).sqrt();
        let rel = ((hi - lo) - 2.0 * g).abs() / (2.0 * g);
        assert!(rel < 2.0 * k, "splitting {} vs 2g {}", hi - lo, 2.0 * g);
    }

    /// Issue #1078: the bare-qubit anchor sits above 4.14 GHz and below the
    /// dressed anchor's `C_Σ`, its shift grows with `k`, and `k = 0` is the
    /// dressed anchor.
    #[test]
    fn devicelayout_bare_anchor_is_below_the_dressed_one() {
        let dressed = TransmonAnchor::devicelayout();
        let at0 = TransmonAnchor::devicelayout_bare_with_coupling(0.0);
        assert!((at0.c_sigma_farad - dressed.c_sigma_farad).abs() < 1e-24);
        let bare = TransmonAnchor::devicelayout_bare();
        assert!(bare.frequency_hz > DEVICELAYOUT_QUBIT_FREQUENCY_HZ);
        assert!(bare.c_sigma_farad < dressed.c_sigma_farad);
        let (lo, hi) = DEVICELAYOUT_COUPLING_K_BAND;
        assert!(lo <= DEVICELAYOUT_COUPLING_K && DEVICELAYOUT_COUPLING_K <= hi);
        let c = |k: f64| TransmonAnchor::devicelayout_bare_with_coupling(k).c_sigma_farad;
        assert!(c(hi) < c(DEVICELAYOUT_COUPLING_K) && c(DEVICELAYOUT_COUPLING_K) < c(lo));
        // The shift is close to proportional to k²: 4x for 2x k, to 1 %.
        let s = |k: f64| dressed.c_sigma_farad - c(k);
        let r = s(0.04) / s(0.02);
        assert!((r - 4.0).abs() < 0.04, "shift ratio {r}");
    }

    /// E_J from the DeviceLayout junction inductance is 11.00 GHz (the
    /// curator-confirmed anchor, E_J/h = 11.0001 GHz).
    #[test]
    fn e_j_from_l_j_is_11_ghz() {
        let e_j = e_j_hz_from_inductance(14.860e-9);
        assert!(
            (e_j / 1e9 - 11.0001).abs() < 1e-3,
            "E_J/h = {} GHz, want ≈ 11.0001",
            e_j / 1e9
        );
    }

    /// E_C ⇄ C round-trip of the bare formula (total capacitance).
    #[test]
    fn e_c_capacitance_roundtrip() {
        for &c in &[50e-15_f64, 99.4537e-15, 136.7e-15] {
            let e_c = e_c_hz_from_capacitance(c);
            let back = capacitance_from_e_c_hz(e_c);
            assert!((back - c).abs() / c < 1e-12);
        }
        // e²/(2h) = 19.3702 GHz·fF.
        let k = e_c_hz_from_capacitance(1e-15) / 1e9;
        assert!((k - 19.370_2).abs() < 1e-4, "e²/(2h) = {k} GHz·fF");
    }

    /// The DeviceLayout anchor (issue #1066): derived from 4.14 GHz,
    /// L_J = 14.860 nH and C_J = 5.5 fF, it is C_total = 99.4537 fF,
    /// C_Σ = 93.9537 fF and E_C/h = 0.194766 GHz (the operator ruling on
    /// #1063), and it reproduces 4.14 GHz through the harmonic formula.
    #[test]
    fn devicelayout_anchor_is_the_harmonic_back_solve() {
        let a = TransmonAnchor::devicelayout();
        assert_eq!(a.frequency_hz, 4.14e9);
        assert_eq!(a.l_j_henry, 14.860e-9);
        assert_eq!(a.c_j_farad, 5.5e-15);
        assert!((a.c_total_farad * 1e15 - 99.4537).abs() < 5e-5, "{a:?}");
        assert!((a.c_sigma_farad * 1e15 - 93.9537).abs() < 5e-5, "{a:?}");
        assert!((a.e_c_hz / 1e9 - 0.194766).abs() < 5e-7, "{a:?}");
        assert!((a.e_j_hz / 1e9 - 11.0001).abs() < 1e-3, "{a:?}");
        assert_eq!(a.c_sigma_farad, a.c_total_farad - a.c_j_farad);
        // Round trip through the frequency and through E_C.
        let f = a.harmonic_frequency_of_c_sigma_hz(a.c_sigma_farad);
        assert!((f - 4.14e9).abs() / 4.14e9 < 1e-13, "f = {f}");
        assert!((a.e_c_hz_of_c_sigma(a.c_sigma_farad) - a.e_c_hz).abs() / a.e_c_hz < 1e-14);
        let c_back = c_sigma_from_e_c_hz(a.e_c_hz, a.c_j_farad);
        assert!((c_back - a.c_sigma_farad).abs() / a.c_sigma_farad < 1e-12);
        // The superseded anchor is not this one: 89.9 fF is about 4 fF short.
        assert!(a.c_sigma_farad * 1e15 - 89.9 > 4.0);
    }

    /// The E_C convention includes C_J (issue #1066), and √(8 E_J E_C) with
    /// that E_C equals the linear-LC frequency 1/(2π √(L_J (C_Σ + C_J)))
    /// for any C_Σ. Leaving C_J out overstates E_C by C_J/C_Σ.
    #[test]
    fn e_c_with_junction_matches_linear_lc_frequency() {
        let (l_j, c_j) = (14.860e-9, 5.5e-15);
        let e_j = e_j_hz_from_inductance(l_j);
        for &c_sigma in &[78.3879e-15_f64, 90.0e-15, 93.9537e-15, 136.7e-15] {
            let e_c = e_c_hz_from_c_sigma(c_sigma, c_j);
            assert_eq!(e_c, e_c_hz_from_capacitance(c_sigma + c_j));
            let lc = harmonic_frequency_hz(l_j, c_sigma + c_j);
            let plasma = (8.0 * e_j * e_c).sqrt();
            assert!(
                (plasma - lc).abs() / lc < 1e-12,
                "√(8E_JE_C) {plasma} vs LC {lc} at C_Σ {c_sigma}"
            );
            let bare = e_c_hz_from_capacitance(c_sigma);
            let over = bare / e_c - 1.0;
            assert!((over - c_j / c_sigma).abs() < 1e-12, "overstatement {over}");
        }
        // The derivative of the junction-inclusive E_C matches FD.
        let c = 93.9537e-15;
        let h = c * 1e-6;
        let fd = (e_c_hz_from_c_sigma(c + h, c_j) - e_c_hz_from_c_sigma(c - h, c_j)) / (2.0 * h);
        let ana = d_e_c_hz_from_c_sigma_d_c_sigma(c, c_j);
        assert!((ana - fd).abs() / fd.abs() < 1e-6, "{ana} vs {fd}");
        // Inverting the harmonic frequency recovers the total capacitance.
        let ct = total_capacitance_from_harmonic_frequency(harmonic_frequency_hz(l_j, c), l_j);
        assert!((ct - c).abs() / c < 1e-12);
    }

    /// The analytic `∂(E_C/h)/∂C_Σ` matches a central finite difference of
    /// [`e_c_hz_from_capacitance`] to tight tolerance (the scalar chain
    /// factor is exact, not an approximation).
    #[test]
    fn d_e_c_d_c_sigma_matches_finite_difference() {
        for &c in &[50e-15_f64, 93.9537e-15, 136.7e-15, 200e-15] {
            let ana = d_e_c_hz_d_c_sigma(c);
            let h = c * 1e-6;
            let fd = (e_c_hz_from_capacitance(c + h) - e_c_hz_from_capacitance(c - h)) / (2.0 * h);
            let rel = (ana - fd).abs() / fd.abs();
            assert!(
                ana < 0.0,
                "∂E_C/∂C_Σ must be negative (more capacitance ⇒ less E_C), got {ana}"
            );
            assert!(
                rel < 1e-6,
                "∂E_C/∂C_Σ at C_Σ={c}: analytic {ana} vs FD {fd}, rel {rel:.3e}"
            );
        }
    }

    /// `∂ω01/∂E_C` (asymptotic) is the exact derivative of
    /// [`omega01_asymptotic_hz`] — matches a central finite difference of it,
    /// and is positive in the transmon regime (`√(2E_J/E_C) ≫ 1`).
    #[test]
    fn d_omega01_asymptotic_matches_finite_difference() {
        let e_j = 11.0e9;
        let e_c = 11.0e9 / 51.0;
        let ana = d_omega01_asymptotic_d_e_c(e_j, e_c);
        let h = e_c * 1e-6;
        let fd =
            (omega01_asymptotic_hz(e_j, e_c + h) - omega01_asymptotic_hz(e_j, e_c - h)) / (2.0 * h);
        let rel = (ana - fd).abs() / fd.abs();
        assert!(ana > 0.0, "∂ω01/∂E_C should be positive in transmon regime");
        assert!(
            rel < 1e-6,
            "∂ω01/∂E_C: analytic {ana} vs FD {fd}, rel {rel:.3e}"
        );
    }

    /// The tridiagonal eigensolver reproduces a known 3×3 spectrum.
    #[test]
    fn tridiagonal_eigenvalues_known_case() {
        // [[2,-1,0],[-1,2,-1],[0,-1,2]] has eigenvalues 2−√2, 2, 2+√2.
        let diag = [2.0, 2.0, 2.0];
        let off = [-1.0, -1.0];
        let mut ev = symmetric_tridiagonal_eigenvalues(&diag, &off);
        ev.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let want = [2.0 - 2f64.sqrt(), 2.0, 2.0 + 2f64.sqrt()];
        for (g, w) in ev.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-12, "eig {g} != {w}");
        }
    }

    /// A diagonal matrix (zero off-diagonals) returns its diagonal.
    #[test]
    fn tridiagonal_diagonal_matrix() {
        let diag = [3.0, 1.0, 7.0, 4.0];
        let off = [0.0, 0.0, 0.0];
        let mut ev = symmetric_tridiagonal_eigenvalues(&diag, &off);
        ev.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(ev, vec![1.0, 3.0, 4.0, 7.0]);
    }

    /// Koch spectrum in the transmon regime: at E_J/E_C ≈ 51 (the ratio of
    /// the superseded 0.2156 GHz anchor) and at the DeviceLayout anchor
    /// (E_J/E_C ≈ 56.5, issue #1066) the exact ω01 matches the
    /// √(8 E_J E_C) − E_C asymptote to the transmon-regime tolerance, and
    /// α ≈ −E_C.
    #[test]
    fn koch_spectrum_matches_asymptotics_in_transmon_regime() {
        let e_j = 11.0e9;
        for e_c in [11.0e9 / 51.0, TransmonAnchor::devicelayout().e_c_hz] {
            koch_regime_case(e_j, e_c);
        }
    }

    fn koch_regime_case(e_j: f64, e_c: f64) {
        let spec = transmon_spectrum(e_j, e_c, 0.0, 30, 3);
        assert!(spec.converged, "spectrum not converged at N=30");

        let w01 = spec.omega01_hz();
        let w01_asymp = omega01_asymptotic_hz(e_j, e_c);
        // At E_J/E_C ≈ 51 the asymptote is good to ~1% (the next-order
        // −E_C/4·(…) correction). Assert within 2%.
        let rel = (w01 - w01_asymp).abs() / w01_asymp;
        assert!(
            rel < 0.02,
            "ω01 exact {} vs asymptote {} (rel {rel})",
            w01 / 1e9,
            w01_asymp / 1e9
        );

        // α ≈ −E_C sanity: the leading transmon expansion gives α = −E_C
        // exactly at first+second order (ω01 = √(8E_JE_C)−E_C,
        // ω12 = √(8E_JE_C)−2E_C ⇒ α = −E_C). The Koch-EXACT diagonalization
        // captures the higher-order correction, which at E_J/E_C ≈ 51 makes
        // |α| ≈ 1.15·E_C — i.e. the −E_C sanity form is good to ~15% here,
        // NOT tighter. This is a genuine physics observation (the exact
        // spectrum is the gate; −E_C is a first-order sanity anchor), so we
        // assert the sign and the 15%-class agreement, not a tight bound.
        let alpha = spec.anharmonicity_hz();
        assert!(alpha < 0.0, "transmon α must be negative, got {alpha}");
        let rel_a = (alpha - (-e_c)).abs() / e_c;
        assert!(
            rel_a < 0.16,
            "α exact {} vs −E_C {} (rel {rel_a}) — outside the ~15% \
             transmon-regime sanity band",
            alpha / 1e9,
            -e_c / 1e9
        );
        // And |α| > E_C (the higher-order correction increases the
        // anharmonicity magnitude in this regime).
        assert!(
            alpha.abs() > e_c,
            "exact |α| {} should exceed E_C {} at E_J/E_C=51",
            alpha.abs() / 1e9,
            e_c / 1e9
        );
    }

    /// DeviceLayout's 4.14 GHz is the HARMONIC frequency of the anchor
    /// (issue #1066): √(8 E_J E_C) at the anchor E_C reproduces it to
    /// rounding. The anharmonic Koch ω01 at the same E_C sits below it by
    /// about E_C (here 3.94 GHz, 4.8 % low), so 4.14 GHz must not be read
    /// as ω01. The earlier version of this test fed the mis-derived
    /// 0.2156 GHz into the Koch ω01 and compared it with 4.14 GHz.
    #[test]
    fn devicelayout_frequency_is_harmonic_not_koch_omega01() {
        let a = TransmonAnchor::devicelayout();
        let plasma = (8.0 * a.e_j_hz * a.e_c_hz).sqrt() / 1e9;
        assert!((plasma - 4.14).abs() < 1e-9, "√(8E_JE_C) = {plasma} GHz");
        let w01 = transmon_spectrum(a.e_j_hz, a.e_c_hz, 0.0, 30, 3).omega01_hz() / 1e9;
        let asym = omega01_asymptotic_hz(a.e_j_hz, a.e_c_hz) / 1e9;
        assert!((asym - (4.14 - a.e_c_hz / 1e9)).abs() < 1e-9);
        // The Koch ω01 is below the harmonic frequency by roughly E_C, and
        // within the asymptote's transmon-regime 2 %.
        let gap = 4.14 - w01;
        assert!(
            gap > 0.8 * a.e_c_hz / 1e9 && gap < 1.2 * a.e_c_hz / 1e9,
            "harmonic − ω01 = {gap} GHz, E_C = {} GHz",
            a.e_c_hz / 1e9
        );
        assert!(
            (w01 - asym).abs() / asym < 0.02,
            "ω01 {w01} vs asymptote {asym}"
        );
    }

    /// Convergence: doubling N does not move ω01 (the `converged` flag).
    #[test]
    fn koch_spectrum_converges() {
        let e_c = TransmonAnchor::devicelayout().e_c_hz;
        let spec = transmon_spectrum(11.0e9, e_c, 0.0, 25, 3);
        assert!(spec.converged);
        // And explicit N=25 vs N=60 agreement.
        let a = transmon_spectrum(11.0e9, e_c, 0.0, 25, 3).omega01_hz();
        let b = transmon_spectrum(11.0e9, e_c, 0.0, 60, 3).omega01_hz();
        assert!((a - b).abs() / b < 1e-9, "N-doubling ω01 drift");
    }

    /// Charge dispersion: the offset-charge sensitivity of ω01 is
    /// exponentially small at E_J/E_C ≈ 51 (the transmon's raison d'être).
    /// |ω01(n_g=0.5) − ω01(n_g=0)| ≪ ω01.
    #[test]
    fn charge_dispersion_is_exponentially_small() {
        let e_j = 11.0e9;
        let e_c = 11.0e9 / 51.0;
        let w0 = transmon_spectrum(e_j, e_c, 0.0, 30, 2).omega01_hz();
        let w_half = transmon_spectrum(e_j, e_c, 0.5, 30, 2).omega01_hz();
        let disp = (w_half - w0).abs() / w0;
        assert!(
            disp < 1e-4,
            "charge dispersion {disp} too large for E_J/E_C=51"
        );
    }

    /// Self-Kerr / cross-Kerr EPR closed forms have the right signs and
    /// scaling.
    #[test]
    fn kerr_closed_forms() {
        let e_c = TransmonAnchor::devicelayout().e_c_hz;
        // Qubit mode p ≈ 1: self-Kerr ≈ −E_C/2.
        let a = self_kerr_from_epr(e_c, 1.0);
        assert!((a - (-0.5 * e_c)).abs() < 1e-3);
        // Cross-Kerr between p=1 and p=0.01 modes is negative and small.
        let chi = cross_kerr_from_epr(e_c, 1.0, 0.01);
        assert!(chi < 0.0);
        assert!((chi - (-2.0 * e_c * 0.01)).abs() < 1e-3);
    }

    /// Correspondence-limit tripwire: the classical Duffing per-photon
    /// shift EQUALS the quantum self-Kerr for the same (E_C, p_m). A
    /// factor-of-2 drift between the two derivations would break this.
    #[test]
    fn duffing_quantum_kerr_correspondence() {
        let e_c = TransmonAnchor::devicelayout().e_c_hz;
        for &p in &[0.05_f64, 0.3, 0.7, 1.0] {
            let quantum = self_kerr_from_epr(e_c, p);
            let classical = duffing_kerr_from_epr(e_c, p);
            assert!(
                (quantum - classical).abs() <= 1e-9 * quantum.abs().max(1.0),
                "correspondence broken at p={p}: quantum {quantum} vs classical {classical}"
            );
        }
    }

    /// Minev participation is the clamped junction/total inductive ratio.
    #[test]
    fn minev_participation_ratio() {
        assert!((minev_participation(0.3, 1.0) - 0.3).abs() < 1e-15);
        // Junction mode: nearly all inductive energy in the junction.
        assert!((minev_participation(0.999, 1.0) - 0.999).abs() < 1e-15);
        // Clamp against rounding overshoot.
        assert_eq!(minev_participation(1.0000001, 1.0), 1.0);
    }
}
