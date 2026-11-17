//! **Report and warning data model of a hybrid wave port** (Epic #778
//! Phase 2 onward): the settings a caller hands a
//! [`HybridWavePort`](super::HybridWavePort), the per-channel /
//! per-frequency / per-port reports a hybrid sweep returns, and the warning
//! taxonomy those reports carry.
//!
//! This is a pure-data leaf of the hybrid-port concern — plain `pub` structs
//! and enums with derives, plus the two `Default` impls that define the
//! shipped accuracy and port settings. It holds no solver state and reads
//! nothing private from [`super::hybrid`], so the dependency flows one way:
//! the sweep drivers ([`super::hybrid`]), the real state machine
//! ([`super::hybrid_state`]), the lossy path ([`super::hybrid_lossy`]) and
//! the line-impedance path ([`super::hybrid_z`]) all fill these types in.
//!
//! Every type here is re-exported from [`super`], so callers keep seeing a
//! single `driven::ports` namespace.

use faer::c64;

use super::hybrid::DEFAULT_MIN_TRACK_OVERLAP;
use super::hybrid_z::{DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD, ImpedanceAccuracy, LineImpedance};
use super::mixed::MixedPortSweepPoint;
use super::wave::WavePortSweepPoint;
use crate::analytic::port_mode_accuracy::{DEFAULT_ACCURACY_THRESHOLD, ModeAccuracy};
use crate::analytic::port_modes::HybridPortOpts;

/// Accuracy-estimate settings of a [`HybridWavePort`](super::HybridWavePort).
#[derive(Debug, Clone, Copy)]
pub struct PortAccuracyOpts {
    /// Relative `β` error above which a propagating channel raises
    /// [`PortWarningKind::AccuracyAboveThreshold`]. Default 0.5 %.
    pub threshold: f64,
    /// Solve a third level (`h/4`) at the first frequency to measure each
    /// channel's convergence rate (used for the Richardson factor and the
    /// resolution hint). `false`: the nominal rate 2. Default `true`.
    pub observe_rate: bool,
    /// Relative error of the attenuation `α = −Im β` above which a
    /// propagating channel of a **lossy** port raises
    /// [`PortWarningKind::AttenuationAccuracyAboveThreshold`] (from
    /// [`HybridChannelReport::alpha_accuracy`]; #819 review). `None`
    /// (default): `α` is estimated and reported but never warned about. The
    /// `geode` CLI sets it to [`Self::threshold`] (#807).
    pub alpha_threshold: Option<f64>,
    /// Relative line-impedance error above which a propagating channel of a
    /// face with floating conductors raises
    /// [`PortWarningKind::ImpedanceAccuracyAboveThreshold`] (from
    /// [`HybridChannelReport::impedance_accuracy`], for the impedance
    /// [`Self::impedance`]; #807 review). Default
    /// [`DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD`] (1 %). `None`: estimated and
    /// reported, never warned about.
    pub impedance_threshold: Option<f64>,
    /// The line impedance [`Self::impedance_threshold`] is checked on (the
    /// one a front end renormalizes to). Default `Z_PI`.
    pub impedance: LineImpedance,
}

impl Default for PortAccuracyOpts {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ACCURACY_THRESHOLD,
            observe_rate: true,
            alpha_threshold: None,
            impedance_threshold: Some(DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD),
            impedance: LineImpedance::PowerCurrent,
        }
    }
}

/// Options of a [`HybridWavePort`](super::HybridWavePort).
#[derive(Debug, Clone, Copy)]
pub struct HybridWavePortOpts {
    /// Evanescent slots terminated beyond the reported channels (not
    /// reported in S; a complex pair takes two). Default 0 (the geometric
    /// path's behaviour: only reported channels are terminated).
    pub n_termination_evanescent: usize,
    /// Smallest tracking overlap accepted, and the level a second candidate
    /// must stay below. Default [`DEFAULT_MIN_TRACK_OVERLAP`].
    pub min_track_overlap: f64,
    /// Per-mode accuracy estimate (default on).
    pub accuracy: Option<PortAccuracyOpts>,
    /// Krylov cap of the mode solve ([`HybridPortOpts::max_krylov`]).
    pub max_krylov: usize,
    /// Residual threshold of the mode solve
    /// ([`HybridPortOpts::residual_tol`]).
    pub residual_tol: f64,
    /// A carried complex pair whose self-pairing conditioning
    /// ([`HybridComplexPair::conditioning`]) is at or below this is treated
    /// as degenerate: the 2×2 block is not formed and the termination window
    /// stops above the pair ([`PortWarningKind::ComplexPairDropped`]).
    ///
    /// [`HybridComplexPair::conditioning`]: crate::analytic::port_modes::HybridComplexPair::conditioning
    /// Default [`crate::analytic::port_modes::PAIR_DEGENERATE_TOL`].
    pub pair_degenerate_tol: f64,
    /// Run the P1 multiplicity verification pass on every face solve
    /// ([`HybridPortOpts::verify_multiplicity`]). Default `true`. When a solve
    /// comes back uncertified the sweep retries once with twice the Krylov
    /// cap, then continues with a [`PortWarningKind::MultiplicityUncertified`]
    /// warning (a missed copy of a repeated eigenvalue would otherwise be
    /// invisible to the completeness guard, which counts the solver's own
    /// output). `false` skips the pass (faster) and always warns.
    pub verify_multiplicity: bool,
    /// **Inverse tripwire** (tests only): drop the `−(j/β)∇_tE_z` term and use
    /// the `E_t`-only flux `f = S_p E_t / √(E_tᵀS_pE_t)`. Physical runs keep
    /// `false`.
    pub transverse_only_flux: bool,
}

impl Default for HybridWavePortOpts {
    fn default() -> Self {
        let p1 = HybridPortOpts::default();
        Self {
            n_termination_evanescent: 0,
            min_track_overlap: DEFAULT_MIN_TRACK_OVERLAP,
            accuracy: Some(PortAccuracyOpts::default()),
            max_krylov: p1.max_krylov,
            residual_tol: p1.residual_tol,
            pair_degenerate_tol: crate::analytic::port_modes::PAIR_DEGENERATE_TOL,
            verify_multiplicity: true,
            transverse_only_flux: false,
        }
    }
}

/// One reported channel of a hybrid port at one frequency.
#[derive(Debug, Clone)]
pub struct HybridChannelReport {
    /// Outgoing-branch `β`.
    pub beta: c64,
    /// `Re β²` (`β²` itself on the real lossless path).
    pub beta_sq: f64,
    /// `Im β²`: `0` on the real path; negative for a passive lossy mode
    /// (#806). The attenuation is `α = −Im β`.
    pub beta_sq_im: f64,
    /// `E_z` energy fraction `1 − η` of the mode.
    pub ez_energy_fraction: f64,
    /// Tracking overlap from the previous (lower) frequency; `None` at the
    /// first frequency.
    pub track_overlap: Option<f64>,
    /// Per-mode accuracy estimate (`None`: disabled, or the refined match
    /// failed — the latter also raises a warning).
    pub accuracy: Option<ModeAccuracy>,
    /// Explicit residual of the mode solve.
    pub residual: f64,
    /// Estimated relative error of the attenuation `α = −Im β` from the same
    /// `h/2` re-solve ([`crate::analytic::lossy_port_modes::lossy_alpha_estimate`]);
    /// `None` on the real path or when unavailable.
    pub alpha_accuracy: Option<f64>,
    /// Line-impedance accuracy estimate (`Z_PI`, and `Z_PV` / `Z_VI` with a
    /// voltage path) of a propagating channel on a face with floating
    /// conductors, from the same `h/2` re-solve with the **observed**
    /// impedance rate ([`ImpedanceAccuracy`]); `None` otherwise or
    /// when unavailable (then
    /// [`PortWarningKind::ImpedanceAccuracyUnavailable`] says why).
    pub impedance_accuracy: Option<ImpedanceAccuracy>,
    /// Round-off floor of [`Self::residual`]
    /// ([`HybridPortMode::residual_floor`]).
    ///
    /// [`HybridPortMode::residual_floor`]: crate::analytic::port_modes::HybridPortMode::residual_floor
    pub residual_floor: f64,
    /// `true` when the mode was accepted at the round-off floor:
    /// `residual_tol < residual ≤ residual_floor`. On strip-graded faces the
    /// floor (∝ `1/k₀²`) sits above `residual_tol` across much of the
    /// practical microstrip band, not only at low frequency (#818 review), so
    /// this is reported per mode at every frequency.
    pub floor_accepted: bool,
    /// Size of the exactly degenerate cluster the channel belongs to (`1`:
    /// a simple eigenvalue). Inside a cluster the channel is a tracked
    /// combination of the cluster's eigenvectors (module docs).
    pub cluster_size: usize,
    /// Line quantities (`Z_PI`, `Z_PV`, `Z_VI`, conductor currents and
    /// voltages) for a propagating channel on a face with floating
    /// conductors; `None` otherwise. Real faces only: a lossy face reports
    /// [`Self::line_lossy`].
    pub line: Option<HybridLineReport>,
    /// The complex line quantities of a propagating channel of a **lossy**
    /// face with floating conductors (Phase 5, #807); `None` otherwise.
    pub line_lossy: Option<HybridComplexLineReport>,
}

/// Line quantities of one propagating hybrid channel (#817), for the
/// characteristic-impedance choice of Phase 5 (#807).
///
/// Per conductor `c` (the face's [`HybridPortFace::conductors`] order): the
/// discrete-Ampère current `I_c` and the path voltage `V_c` (shield →
/// conductor along [`FaceConductor::voltage_path`]), with the definitions of
/// [`crate::analytic::port_modes::mode_line_quantities`]. With several
/// conductors the impedances are the per-line modal impedances
///
/// ```text
///   Z_PI = 2P / Σ_c |I_c|²,   Z_PV = Σ_c |V_c|² / (2P),   Z_VI = √(Z_PI · Z_PV),
/// ```
///
/// which reduce to the single-conductor `2P/|I|²`, `|V|²/2P`, `|V/I|` and,
/// for the even / odd mode of a symmetric pair (`|I₁| = |I₂|`), are the
/// usual even / odd line impedances `Z_e`, `Z_o`. All three coincide
/// quasi-statically and separate as dispersion grows; none of them is the
/// #775 wave impedance `Z_TE = η₀k₀/β`.
///
/// `Z_PI` is contour-independent. `Z_PV` and `Z_VI` are not: a hybrid mode
/// is not TEM, so the path voltage depends on the voltage path, and the
/// dependence grows with frequency. The automatic path is the shortest
/// free-edge path from the shield and ends at the nearest point of the
/// conductor (for a microstrip, the strip **edge**, not its centre). On the
/// P3b microstrip at `k₀h = 0.2` this gives `Z_PV` 73.15 Ω against 74.00 Ω
/// along the P3a centre path, and `Z_PV / Z_PI` drifts from 1.013 to 1.109
/// over `k₀h = 0.05 … 0.2`.
///
/// [`FaceConductor::voltage_path`]: super::FaceConductor::voltage_path
/// [`HybridPortFace::conductors`]: super::HybridPortFace::conductors
#[derive(Debug, Clone)]
pub struct HybridLineReport {
    /// Modal power `P = xᵀBx/(2k₀η₀β)` in the mode's P1 normalization.
    pub power: f64,
    /// Signed conductor currents `I_c` (same normalization).
    pub currents: Vec<f64>,
    /// Signed path voltages `V_c`; `None` where no path exists.
    pub voltages: Vec<Option<f64>>,
    /// Power–current impedance (ohms).
    pub z_pi: f64,
    /// Power–voltage impedance (ohms); `None` if a voltage is missing.
    pub z_pv: Option<f64>,
    /// Voltage–current impedance (ohms); `None` if a voltage is missing.
    pub z_vi: Option<f64>,
    /// `true` when every conductor's net current cancels to floating-point
    /// round-off (issue #953): a TE / TM waveguide mode of the face (such
    /// as a coax TE₁₁), not a line mode. Then `Z_PI` and `Z_VI` are
    /// **undefined** — `z_pi` and `z_vi` hold ratios of round-off (about
    /// `1e31 Ω` on the coax) and must not be used — no line-impedance
    /// accuracy is estimated, and a
    /// [`PortWarningKind::NoNetConductorCurrent`] says so. The test is per
    /// conductor against the a-priori round-off bound of its discrete-Ampère
    /// sum (no tuned constant), so an odd coupled mode (opposite, non-zero
    /// currents) and a mode near cutoff (small but real current) keep their
    /// impedance.
    pub no_net_current: bool,
}

/// Line quantities of one propagating channel of a **lossy** hybrid port
/// (Phase 5, #807): the unconjugated (reciprocal) counterparts of
/// [`HybridLineReport`], with the definitions of Phase 5's operator decision.
///
/// With the channel normalized `zᵀBz = β²` (complex), the modal pseudo-power
/// is `P = zᵀBz/(2k₀η₀β)`, the conductor currents `I_c` are the discrete
/// Ampère sums with the **complex** `ε` (the displacement term carries
/// `T_{ε″}`), the path voltages `V_c = Σ ±ẽ_t/β`, and
///
/// ```text
///   Z_PI = 2P / Σ_c I_c²,   Z_PV = Σ_c V_c² / (2P),   Z_VI = √(Z_PI · Z_PV)
/// ```
///
/// (unconjugated squares, principal root). For a TEM line this is
/// `Z = √((R + jωL)/(G + jωC))`, the complex characteristic impedance a
/// Touchstone renormalization needs; in the lossless limit it is exactly the
/// real [`HybridLineReport`] (`P`, `I_c`, `V_c` real). Like the real ones,
/// `Z_PV` and `Z_VI` depend on the voltage path.
#[derive(Debug, Clone)]
pub struct HybridComplexLineReport {
    /// Modal pseudo-power `P = zᵀBz/(2k₀η₀β)` (complex).
    pub power: c64,
    /// Signed conductor currents `I_c` (complex).
    pub currents: Vec<c64>,
    /// Path voltages `V_c`; `None` where no path exists.
    pub voltages: Vec<Option<c64>>,
    /// Power–current impedance (ohms, complex).
    pub z_pi: c64,
    /// Power–voltage impedance (ohms, complex); `None` if a voltage is
    /// missing.
    pub z_pv: Option<c64>,
    /// Voltage–current impedance (ohms, complex); `None` if a voltage is
    /// missing.
    pub z_vi: Option<c64>,
    /// No net conductor current (issue #953): `z_pi` and `z_vi` are
    /// undefined. As [`HybridLineReport::no_net_current`].
    pub no_net_current: bool,
}

/// A hybrid port at one frequency.
#[derive(Debug, Clone)]
pub struct HybridPortPointReport {
    /// Frequency.
    pub omega: f64,
    /// Reported channels (channel order).
    pub channels: Vec<HybridChannelReport>,
    /// Propagating modes of the face at this ω.
    pub n_propagating: usize,
    /// Termination-only real evanescent channels.
    pub termination_real: usize,
    /// Complex pairs terminated as 2×2 blocks.
    pub termination_pairs: usize,
    /// [`HybridSolveDiagnostics::multiplicity_certified`] of the face solve
    /// used (after the retry, if one ran).
    ///
    /// [`HybridSolveDiagnostics::multiplicity_certified`]: crate::analytic::port_modes::HybridSolveDiagnostics::multiplicity_certified
    pub multiplicity_certified: bool,
    /// `true` when the first solve was uncertified and the sweep re-solved
    /// with twice the Krylov cap.
    pub multiplicity_retried: bool,
    /// Missed copies the verification pass added.
    pub repeated_copies: usize,
    /// Exactly degenerate clusters among the face's returned modes.
    pub degenerate_clusters: usize,
    /// Face modes accepted at the residual round-off floor (all returned
    /// modes, reported or not).
    pub floor_accepted: usize,
}

/// Per-port diagnostics of a hybrid port over a sweep.
#[derive(Debug, Clone)]
pub struct HybridPortReport {
    /// Index of the port in the wave-port list.
    pub port: usize,
    /// Face mesh size `h` (largest edge).
    pub mesh_size: f64,
    /// One entry per frequency, in input order.
    pub points: Vec<HybridPortPointReport>,
    /// Observed per-channel `β` convergence rates (`h, h/2, h/4` at the
    /// first frequency), when measured.
    pub observed_rates: Option<Vec<f64>>,
    /// Floating conductors of the face ([`HybridPortFace::conductors`]).
    ///
    /// [`HybridPortFace::conductors`]: super::HybridPortFace::conductors
    pub n_conductors: usize,
}

/// What a [`PortWarning`] is about.
#[derive(Debug, Clone, PartialEq)]
pub enum PortWarningKind {
    /// A mesh-induced complex pair was terminated as a 2×2 block.
    ComplexPairTerminated {
        /// Mode indices of the pair (descending `Re β²` order of the face's
        /// modes) at the first frequency it appeared.
        mode_indices: [usize; 2],
        /// `β²` of the pair (the `Im < 0` member) there.
        beta_sq: c64,
        /// First frequency it appeared at.
        omega: f64,
        /// Number of frequencies with a terminated pair.
        n_omegas: usize,
        /// Predicted face resolution at which the pair resolves into two
        /// real modes (Krein-collision fit over `h, h/2`; `None`: no
        /// estimate, e.g. with the accuracy estimate disabled).
        h_hint: Option<f64>,
    },
    /// A degenerate pair (`zᵀBz ≈ 0`) stopped the termination window.
    ComplexPairDropped {
        /// Mode indices of the pair.
        mode_indices: [usize; 2],
        /// Frequency.
        omega: f64,
        /// Termination slots that were requested but not filled.
        unfilled_slots: usize,
    },
    /// A propagating channel's estimated `β` error exceeds the threshold.
    AccuracyAboveThreshold {
        /// Channel index within the port.
        channel: usize,
        /// Frequency of the worst estimate.
        omega: f64,
        /// Worst estimate.
        estimate: f64,
        /// Threshold.
        threshold: f64,
        /// Current face mesh size.
        h: f64,
        /// Face mesh size meeting the threshold (observed/nominal rate).
        h_required: f64,
        /// `E_z` energy fraction of the mode there.
        ez_energy_fraction: f64,
    },
    /// A lossy propagating channel's estimated attenuation error exceeds
    /// [`PortAccuracyOpts::alpha_threshold`] (#819 review; off by default).
    AttenuationAccuracyAboveThreshold {
        /// Channel index within the port.
        channel: usize,
        /// Frequency of the worst estimate.
        omega: f64,
        /// Worst estimate of the relative `α` error.
        estimate: f64,
        /// Threshold.
        threshold: f64,
        /// Current face mesh size.
        h: f64,
        /// Face mesh size meeting the threshold (`h·(τ/est)^{1/p}` at the
        /// channel's observed or nominal rate `p`).
        h_required: f64,
    },
    /// A propagating channel's estimated line-impedance error (of
    /// [`PortAccuracyOpts::impedance`]) exceeds
    /// [`PortAccuracyOpts::impedance_threshold`] (#807 review). Covers mesh
    /// error only: the shield's effect on a shielded line's impedance is
    /// physics and is not flagged.
    ImpedanceAccuracyAboveThreshold {
        /// Channel index within the port.
        channel: usize,
        /// Frequency of the worst estimate.
        omega: f64,
        /// Which impedance.
        impedance: LineImpedance,
        /// The coarse (reported) impedance there (ohms).
        z: c64,
        /// Worst estimate of the relative impedance error.
        estimate: f64,
        /// Threshold.
        threshold: f64,
        /// Convergence rate used.
        rate: f64,
        /// `true`: [`Self::ImpedanceAccuracyAboveThreshold::rate`] was
        /// observed (`h, h/2, h/4`); `false`: the conservative singular rate.
        rate_observed: bool,
        /// Uniform refinement factor `(estimate/threshold)^{1/rate}` that
        /// meets the threshold.
        refine_factor: f64,
        /// Current face mesh size (largest edge).
        h: f64,
        /// `h / refine_factor`.
        h_required: f64,
        /// Current smallest face edge (the conductor-edge cell on a graded
        /// face).
        h_min: f64,
        /// `h_min / refine_factor`: the conductor-edge cell size to grade
        /// to.
        h_min_required: f64,
    },
    /// The line-impedance estimate of a propagating channel on a face with
    /// floating conductors could not be formed (refined solve failed, no
    /// refined match, a degenerate cluster, …); the impedance is reported
    /// without an error estimate.
    ImpedanceAccuracyUnavailable {
        /// Channel index within the port.
        channel: usize,
        /// First frequency it was unavailable at.
        omega: f64,
        /// Why.
        reason: String,
    },
    /// A propagating channel on a face with floating conductors carries **no
    /// net conductor current**: every conductor's current cancels to
    /// floating-point round-off ([`HybridLineReport::no_net_current`], issue
    /// #953). It is a TE / TM waveguide mode of the face (such as a coax
    /// TE₁₁), not a line mode, so `Z_PI = 2P/Σ|I|²` and `Z_VI` are undefined
    /// and no line-impedance accuracy is estimated for it. Not a mesh
    /// problem: refining does not give it a current. Raised once per channel
    /// (the first frequency) whether or not the accuracy estimate is on.
    NoNetConductorCurrent {
        /// Channel index within the port.
        channel: usize,
        /// First frequency it was seen at.
        omega: f64,
    },
    /// The refined counterpart of a channel could not be matched, or the
    /// refined (`h/2`) solve failed; the sweep continued without the estimate.
    AccuracyUnavailable {
        /// Channel index within the port.
        channel: usize,
        /// First frequency it failed at.
        omega: f64,
    },
    /// The face solve could not certify that every copy of a repeated
    /// eigenvalue was found (#817), even after a retry with twice the Krylov
    /// cap, or the verification pass was switched off. The sweep continued:
    /// S is right unless a copy is actually missing, in which case that
    /// channel is unterminated.
    MultiplicityUncertified {
        /// First frequency.
        omega: f64,
        /// Number of frequencies affected.
        n_omegas: usize,
        /// Whether the verification pass ran (`false`: switched off).
        verified: bool,
    },
    /// A tracked exactly degenerate cluster split into distinct modes at
    /// this frequency; its channels were assigned inside the captured
    /// subspace by largest overlap (#817).
    ClusterSplit {
        /// Channel indices of the cluster.
        channels: Vec<usize>,
        /// Frequency.
        omega: f64,
        /// Smallest principal cosine between the old and the new subspace.
        min_cosine: f64,
    },
    /// A degenerate cluster of reported channels has no canonical basis
    /// (fewer conductors than modes, or conductor currents that do not
    /// separate them); the solver's B-orthonormal basis is used and kept
    /// consistent across the sweep by subspace tracking (#817).
    NonCanonicalClusterBasis {
        /// Channel indices of the cluster.
        channels: Vec<usize>,
        /// First frequency.
        omega: f64,
        /// Why no canonical basis was formed.
        reason: String,
    },
}

/// A report-level warning of a hybrid port (never an error).
#[derive(Debug, Clone)]
pub struct PortWarning {
    /// Index of the port in the wave-port list.
    pub port: usize,
    /// Structured payload.
    pub kind: PortWarningKind,
    /// Human-readable message.
    pub message: String,
}

/// Result of [`solve_wave_port_spec_sweep_with_mode`](super::solve_wave_port_spec_sweep_with_mode).
#[derive(Debug, Clone)]
pub struct WavePortSpecSweep {
    /// One point per frequency, input order (the wave-sweep layout).
    pub points: Vec<WavePortSweepPoint>,
    /// One report per hybrid port.
    pub hybrid: Vec<HybridPortReport>,
    /// Report-level warnings.
    pub warnings: Vec<PortWarning>,
}

/// Result of [`solve_mixed_port_spec_sweep_with_mode`].
///
/// [`solve_mixed_port_spec_sweep_with_mode`]: super::solve_mixed_port_spec_sweep_with_mode
#[derive(Debug, Clone)]
pub struct MixedPortSpecSweep {
    /// One point per frequency, input order (the mixed-sweep layout).
    pub points: Vec<MixedPortSweepPoint>,
    /// One report per hybrid port.
    pub hybrid: Vec<HybridPortReport>,
    /// Report-level warnings.
    pub warnings: Vec<PortWarning>,
}
