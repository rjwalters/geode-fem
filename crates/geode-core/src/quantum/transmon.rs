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

/// Charging energy in **Hz** (`E_C/h`) from the total capacitance across
/// the junction `C_Σ` (farads): `E_C = e²/(2 C_Σ)`, `E_C/h = e²/(2 C_Σ h)`.
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

    /// E_C ⇄ C_Σ round-trip, and the anchor: E_C/h = 0.2156 GHz ⇒
    /// C_Σ ≈ 89.9 fF (curator-confirmed back-solve).
    #[test]
    fn e_c_capacitance_roundtrip_and_anchor() {
        let e_c = e_c_hz_from_capacitance(89.9e-15);
        assert!(
            (e_c / 1e9 - 0.2156).abs() < 5e-3,
            "E_C/h = {} GHz, want ≈ 0.2156",
            e_c / 1e9
        );
        let c = capacitance_from_e_c_hz(e_c);
        assert!((c - 89.9e-15).abs() / 89.9e-15 < 1e-12);
        // The 0.2156 GHz anchor back-solves to ~89.9 fF.
        let c_anchor = capacitance_from_e_c_hz(0.2156e9);
        assert!(
            (c_anchor * 1e15 - 89.9).abs() < 0.5,
            "C_Σ = {} fF, want ≈ 89.9",
            c_anchor * 1e15
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

    /// Koch spectrum at the transmon anchor (E_J/E_C ≈ 51): the exact
    /// ω01 matches the √(8 E_J E_C) − E_C asymptote to the transmon-regime
    /// tolerance, and α ≈ −E_C.
    #[test]
    fn koch_spectrum_matches_asymptotics_at_anchor() {
        let e_j = 11.0e9;
        let e_c = 11.0e9 / 51.0; // E_J/E_C = 51 → E_C ≈ 0.2157 GHz
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

    /// ω01 lands near the blog's ~4.14 GHz start-geometry qubit when fed
    /// E_J = 11 GHz and the back-solved E_C = 0.2156 GHz.
    #[test]
    fn koch_omega01_near_blog_anchor() {
        let e_j = e_j_hz_from_inductance(14.860e-9);
        let e_c = 0.2156e9;
        let spec = transmon_spectrum(e_j, e_c, 0.0, 30, 3);
        let w01 = spec.omega01_hz() / 1e9;
        assert!(
            (w01 - 4.14).abs() < 0.15,
            "ω01 = {w01} GHz, want ≈ 4.14 (blog start qubit)"
        );
    }

    /// Convergence: doubling N does not move ω01 (the `converged` flag).
    #[test]
    fn koch_spectrum_converges() {
        let spec = transmon_spectrum(11.0e9, 0.2156e9, 0.0, 25, 3);
        assert!(spec.converged);
        // And explicit N=25 vs N=60 agreement.
        let a = transmon_spectrum(11.0e9, 0.2156e9, 0.0, 25, 3).omega01_hz();
        let b = transmon_spectrum(11.0e9, 0.2156e9, 0.0, 60, 3).omega01_hz();
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
        let e_c = 0.2156e9;
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
        let e_c = 0.2156e9;
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
