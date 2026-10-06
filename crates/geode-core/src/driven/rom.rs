//! Adaptive **fast frequency sweep** via a Galerkin projection reduced-order
//! model (PROM) with greedy snapshot sampling (Epic #475, issue #603;
//! Palace adaptive-fast-frequency-sweep parity).
//!
//! # The reduced-order model
//!
//! [`DrivenOperator`]'s `assemble_a_at` forms the frequency-domain operator
//! (module `exp(+jωt)` convention)
//!
//! ```text
//! A(ω) = K − ω²M + iω C(σ) + Σ_p (iω/Z_p) S_p + Σ_Γ c_Γ(ω) S_Γ
//! ```
//!
//! by linear combination of **ω-independent** value tensors, and the RHS
//!
//! ```text
//! b(ω) = iω (rhs_re + i·rhs_im) + Σ_p (2iω/Z_p)(V_inc/ℓ) f_p = ω · b̂
//! ```
//!
//! is **exactly linear in ω** (both the volume-source moments and the
//! matched-source port drive carry a single `iω` prefactor), so one fixed
//! vector `b̂` describes the drive across the whole band.
//!
//! The PROM collects full-order **snapshot solves** `x_s = A(ω_s)⁻¹ b(ω_s)`
//! at a few greedily chosen frequencies, orthonormalizes them into a complex
//! basis `V ∈ ℂ^{n×k}` (modified Gram–Schmidt with one re-orthogonalization
//! pass, standard Hermitian inner product), and projects **once**:
//!
//! ```text
//! K_r = VᴴKV,  M_r = VᴴMV,  C_r = VᴴCV,  S_{p,r} = VᴴS_pV,  b̂_r = Vᴴb̂.
//! ```
//!
//! Note the port-admittance masses `S_p` are stored **separately** from
//! `C(σ)` in [`DrivenOperator`] and are projected as their own family —
//! `3 + n_ports` projected matrices, not 3. Missing them would produce a
//! PROM that never matches the dense sweep on any port-driven fixture.
//!
//! Every subsequent frequency costs one **dense k×k** solve
//! `A_r(ω) x_r = ω b̂_r` with `A_r(ω) = K_r − ω²M_r + iωC_r + Σ_p (iω/Z_p)
//! S_{p,r}`, plus the port readouts on the reconstructed `x ≈ V x_r` — no
//! sparse factorization.
//!
//! # Greedy sampling and the residual indicator
//!
//! Refinement is driven by the **computable** relative residual of the ROM
//! solution against the *full-order* operator,
//!
//! ```text
//! η(ω) = ‖A(ω) V x_r(ω) − b(ω)‖₂ / ‖b(ω)‖₂,
//! ```
//!
//! evaluated over the candidate grid using the cached matrix–basis products
//! `KV`, `MV`, `CV`, `S_pV` (an `O(nk)` linear combination per frequency —
//! no sparse re-assembly, no factorization). `η` is the true residual of the
//! reduced solution, so it is zero (to roundoff) at snapshot frequencies and
//! upper-bounds the solution error only through `‖A(ω)⁻¹‖`; the integration
//! test logs `η` against the true error vs the dense sweep to demonstrate
//! the correlation honestly.
//!
//! **Deterministic selection**: the seed snapshots are the band endpoints
//! plus the grid point nearest the band midpoint (ties toward the lower
//! frequency), processed in ascending frequency order. Each greedy step adds
//! the not-yet-sampled grid frequency with the **largest** indicator,
//! iterating candidates in ascending frequency order with a strict `>`
//! comparison — so exact ties resolve to the **lowest** frequency. Two
//! builds over the same grid and settings select identical snapshots.
//! Iteration stops when the worst indicator over the grid reaches
//! [`RomSettings::tolerance`], when [`RomSettings::max_snapshots`] full-order
//! solves have been spent, or when the candidate grid is exhausted; the
//! achieved worst residual is always reported ([`DrivenRom::worst_residual`]),
//! converged or not.
//!
//! # Impedance surfaces (issue #708)
//!
//! Leontovich / Silver-Müller / London surfaces (`Σ_Γ c_Γ(ω) S_Γ` above)
//! carry a scalar coefficient `c_Γ(ω) = iω/Z_s(ω)` that is **not**
//! polynomial in `iω` (`∝ √ω(1+i)` for a good conductor), but each `S_Γ`
//! is still a fixed real matrix: it projects **once** (`S_{Γ,r} =
//! VᴴS_ΓV`, cached products `S_ΓV`) and only the scalar is re-evaluated
//! per ω ([`crate::driven::solve::SurfaceImpedanceModel::weak_coefficient`],
//! exactly as `assemble_a_at` does). The Galerkin projection needs no
//! polynomial structure — only fixed matrices with scalar ω-coefficients —
//! and the greedy indicator stays the true full-order residual, so the
//! tolerance certificate is unchanged. (v1 of #603 rejected surfaces; the
//! `geode driven` spiral benchmark carries Leontovich copper.) Conductor
//! **surface roughness** (issue #758,
//! [`crate::driven::solve::SurfaceImpedanceModel::RoughConductor`]) is
//! covered by the same argument: it multiplies `Z_s(ω)` by a real scalar
//! `K(ω)` (Hammerstad `atan` / Huray rational in the skin depth), which
//! only changes the per-ω scalar `c_Γ(ω)`, never `S_Γ`.
//!
//! # Multi-port drives (issue #708)
//!
//! [`RomDrive::PerPort`] builds one PROM for all `N` S-parameter
//! excitations: each snapshot frequency costs **one** factorization and
//! `N` back-solves, every solution joins the basis, and the indicator is
//! the worst over the excitations ([`DrivenRom::evaluate_excitations`]).
//!
//! # Wave (modal) ports (issue #774)
//!
//! [`DrivenRom::build_with_wave_ports`] adds wave ports to the lumped
//! families. Wave channel `q` contributes `j·y_q(ω)·f_q f_qᵀ` to the
//! operator (the dense mixed sweep's rank-`N_w` SMW update), with `f_q`
//! the real interior modal flux and `y_q = β_q/μ_t` the admittance factor
//! ([`crate::driven::ports::WavePort::admittance`], issue #777). Each is a
//! **rank-1 family** projected once: with `u_q[c] = f_qᵀ v_c` (a
//! `k`-vector, no `n×k` product),
//!
//! ```text
//! Vᴴ (f fᵀ) V = conj(u_q) u_qᵀ          (the operator is complex-symmetric)
//! ```
//!
//! and only the scalar `j·y_q(ω)` is re-evaluated per ω. `y(ω)` is **not
//! affine** in ω (a square root with a branch point at cutoff), but — as
//! for the surfaces above — it is a scalar on a fixed matrix, so the
//! Galerkin projection is exact in it and `x(ω)` stays smooth jointly in
//! `(ω, y)`. The wave drive is `b_q(ω) = c_q(ω)·f_q` with `c_q = 2j·y_q·
//! a_inc,q` (the lumped drives keep `c = ω`); the indicator normalizes by
//! `|c_q|·‖f_q‖` and **includes** the modal terms (`f_q·(u_qᵀ x_r)`,
//! `O(n)` each), so it is the same full-order residual the dense mixed /
//! wave sweeps report. Snapshots factor `A_base(ω)` and apply the dense
//! sweep's SMW post-step (shared code), and only the `N_l + N_w`
//! excitation solutions join the basis.
//! [`DrivenRom::evaluate_scattering`] reads the power-wave S-matrix with
//! the dense sweep's own readout (lumped `V_inc/√R`, wave `a·√y/√ω`) on
//! `x = V x_r`.
//!
//! **Near cutoff** the pure-wave guide is nearly singular (`y → 0`: the
//! port stops absorbing, and the cutoff mode satisfies the natural port
//! BC), so `‖A⁻¹‖` amplifies `η` into S error there; exactly at a lossless
//! cutoff the wave drive vanishes and the S column is non-finite, as in
//! the dense sweep. **Dispersive** fills stay out of scope: `y(ω)` alone
//! would project, but the matching volume `M(ω)` inside the guide does
//! not.
//!
//! # Out of scope and follow-on hooks
//!
//! - Matched UPML **re-assembled per ω** (the physical open-boundary
//!   stretch, `∝ 1/k₀`) has ω-dependent matrices and cannot be projected
//!   once.
//! - Hermite / derivative-augmented snapshots, rigorous error certificates,
//!   and sweep-level adjoints are follow-ons. The struct stores the reduced
//!   matrices explicitly so `∂A_r/∂ω = −2ωM_r + iC_r + Σ_p (i/Z_p) S_{p,r}`
//!   is analytically available — the cheap-`∂S/∂ω` (group delay) and
//!   parameter-continuation hooks need no rework, only new methods.
//!
//! Matched-UPML / anisotropic materials assembled **once** (ω-independent
//! complex `K`/`M` values) are fine — the PROM only requires the operator to
//! be polynomial in `iω` with fixed matrices. (The committed patch-antenna
//! tests rebuild their UPML materials per ω and are therefore *not*
//! PROM-compatible fixtures; see issue #603.)

use burn::tensor::backend::Backend;
use faer::c64;

use crate::driven::extraction::PortCircuit;
use crate::driven::ports::mixed::{
    ModalChannel, ModalSmw, PowerWeights, channel_admittances, dot_t, excitation_rhs,
    incident_wave, interior_fluxes, modal_channels, s_column,
};
use crate::driven::ports::{LumpedPort, WavePort, WavePortSpec};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
    SurfaceImpedanceModel,
};
use crate::mesh::TetMesh;

/// Errors from the PROM fast-sweep path.
#[derive(Debug, thiserror::Error)]
pub enum RomError {
    /// A structurally invalid sweep request (empty grid, non-finite or
    /// non-positive frequency, zero snapshot budget, …).
    #[error("invalid PROM parameter: {0}")]
    InvalidParameter(String),
    /// The reduced dense system was singular at a requested evaluation
    /// frequency (the greedy loop treats this as an infinite residual and
    /// samples there instead; seeing this from [`DrivenRom::evaluate`]
    /// means the basis is degenerate at this ω).
    #[error("reduced {order}×{order} PROM system is singular at ω = {omega}")]
    ReducedSolveSingular { order: usize, omega: f64 },
    /// A full-order snapshot solve failed.
    #[error(transparent)]
    Driven(#[from] DrivenError),
}

/// Greedy-PROM stopping knobs.
#[derive(Debug, Clone, Copy)]
pub struct RomSettings {
    /// Stop when the worst residual indicator `η(ω)` over the candidate
    /// grid drops to this value.
    pub tolerance: f64,
    /// Hard budget on full-order snapshot solves (greedy stops here even
    /// if `tolerance` is not reached; the result then reports
    /// `converged() == false` with the honest achieved residual).
    pub max_snapshots: usize,
}

impl Default for RomSettings {
    /// `tolerance = 1e-8`, `max_snapshots = 20`.
    fn default() -> Self {
        Self {
            tolerance: 1e-8,
            max_snapshots: 20,
        }
    }
}

/// One frequency point of a PROM sweep — the reduced-solve analog of
/// [`crate::driven::extraction::SweepPoint`], with the computable
/// residual indicator in place of a solver residual.
#[derive(Debug, Clone)]
pub struct RomSweepPoint {
    /// Frequency `ω ≡ k₀` (natural units, as in [`crate::driven`]).
    pub omega: f64,
    /// Full-order relative residual `‖A(ω)Vx_r − b(ω)‖ / ‖b(ω)‖` of the
    /// reconstructed solution at this frequency.
    pub residual_indicator: f64,
    /// Per-port circuit quantities on `x ≈ V x_r`, in operator port
    /// order — read out with the **same** flux functional and Thevenin
    /// relation as the dense sweep.
    pub ports: Vec<PortCircuit>,
}

/// Result of a [`rom_frequency_sweep`]: the per-frequency points plus the
/// greedy diagnostics needed for honest reporting.
#[derive(Debug, Clone)]
pub struct RomSweepReport {
    /// One entry per requested frequency, in request order.
    pub points: Vec<RomSweepPoint>,
    /// Snapshot frequencies, in greedy selection order (seeds first,
    /// ascending; then worst-residual picks). Its length is the number of
    /// full-order solves spent.
    pub snapshot_omegas: Vec<f64>,
    /// Whether the worst residual indicator reached
    /// [`RomSettings::tolerance`] before the snapshot budget ran out.
    pub converged: bool,
    /// Worst residual indicator over the candidate grid at termination —
    /// the honest achieved bar, converged or not.
    pub worst_residual: f64,
}

/// Which right-hand side(s) a [`DrivenRom`] is built for (issue #708).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RomDrive {
    /// The operator's **baked** drive — volume current source plus every
    /// lumped port at its `v_inc` — as one right-hand side: the
    /// [`crate::driven::solve::FactoredDrivenOperator::solve`] solution
    /// (the historical issue-#603 behavior, and the
    /// [`crate::driven::extraction::driven_frequency_sweep`] analog).
    #[default]
    Baked,
    /// One right-hand side **per lumped port**: port `j` alone driven at
    /// its `v_inc`, every other port a passive resistive termination —
    /// the S-parameter excitations of
    /// [`crate::driven::solve::FactoredDrivenOperator::solve_excited`]
    /// (the [`crate::driven::extraction::s_parameter_frequency_sweep`]
    /// analog). One factorization per snapshot frequency serves all `N`
    /// excitations (a **block** snapshot): the basis grows by up to `N`
    /// columns per snapshot, and the greedy indicator is the worst over
    /// the excitations. Read the per-excitation port quantities with
    /// [`DrivenRom::evaluate_excitations`].
    PerPort,
}

/// One frequency point of a [`DrivenRom`] evaluated for **every** drive
/// ([`DrivenRom::evaluate_excitations`]).
#[derive(Debug, Clone)]
pub struct RomExcitationPoint {
    /// Frequency `ω ≡ k₀` (natural units).
    pub omega: f64,
    /// Worst full-order relative residual indicator over the drives.
    pub residual_indicator: f64,
    /// `excitations[d][k]`: port `k`'s circuit quantities under drive
    /// `d`. For [`RomDrive::Baked`] there is one drive (every port at its
    /// `v_inc`); for [`RomDrive::PerPort`] drive `d` is port `d` alone,
    /// and a passive port's current is `I = −V/R` (`V_inc = 0`), exactly
    /// as in the dense S-parameter sweep's per-excitation readback.
    pub excitations: Vec<Vec<PortCircuit>>,
}

/// One frequency point of a wave-port / mixed-port [`DrivenRom`]
/// ([`DrivenRom::evaluate_scattering`], issue #774): the power-wave
/// S-matrix in the channel order and normalization of
/// [`crate::driven::ports::solve_mixed_port_sweep_with_mode`].
#[derive(Debug, Clone)]
pub struct RomScatteringPoint {
    /// Frequency `ω ≡ k₀` (natural units).
    pub omega: f64,
    /// Worst full-order relative residual indicator over the
    /// excitations (modal terms included).
    pub residual_indicator: f64,
    /// Row-major `n_ports × n_ports` power-wave S-matrix: lumped ports
    /// first, then wave channels port-major, mode-minor.
    pub s: Vec<c64>,
    /// Modal `β(ω)` of each wave channel (wave-channel order) — the same
    /// call as the dense sweep, so bit-equal to it.
    pub beta: Vec<c64>,
    /// Total number of S-matrix ports `N_l + Σ_p K_p`.
    pub n_ports: usize,
    /// Number of lumped ports `N_l` (the leading block of `s`).
    pub n_lumped: usize,
    /// Per-wave-port mode count `K_p`.
    pub port_mode_counts: Vec<usize>,
}

/// Result of a [`rom_mixed_port_sweep`]: per-frequency scattering points
/// plus the greedy diagnostics.
#[derive(Debug, Clone)]
pub struct RomScatteringReport {
    /// One entry per requested frequency, in request order.
    pub points: Vec<RomScatteringPoint>,
    /// Snapshot frequencies, in greedy selection order.
    pub snapshot_omegas: Vec<f64>,
    /// Whether the worst residual indicator reached the tolerance.
    pub converged: bool,
    /// Worst residual indicator over the candidate grid at termination.
    pub worst_residual: f64,
    /// Reduced dimension `k`.
    pub reduced_order: usize,
}

/// One right-hand side of a [`DrivenRom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drive {
    /// Volume source + every lumped port at its `v_inc` (`b = ω·b̂`).
    Baked,
    /// Lumped port `j` alone (`b = ω·b̂`).
    Lumped(usize),
    /// Wave channel `q` alone (`b = 2j·y_q(ω)·a_inc,q · f_q`).
    Wave(usize),
}

/// The wave (modal) channels of a [`DrivenRom`] built with
/// [`DrivenRom::build_with_wave_ports`].
struct ModalData<'a> {
    /// Borrowed from [`DrivenRom::build_with_wave_ports`], owned (cloned
    /// geometric ports) from [`DrivenRom::build_with_wave_port_specs`].
    wave: std::borrow::Cow<'a, [WavePort]>,
    /// Channels port-major, mode-minor, with full-length fluxes.
    channels: Vec<ModalChannel>,
    /// Interior-filtered fluxes `f_q` (real, stored complex).
    fluxes_int: Vec<Vec<c64>>,
    /// `u_q[c] = f_qᵀ v_c` per channel (length `k`): the rank-1 family
    /// projects as `Vᴴ f fᵀ V = conj(u) uᵀ`.
    u: Vec<Vec<c64>>,
    pec_mask: Vec<bool>,
    port_mode_counts: Vec<usize>,
}

/// A built Galerkin PROM over a [`DrivenOperator`]: the orthonormal basis
/// `V`, the cached matrix–basis products, and the projected reduced
/// matrices. Construct with [`DrivenRom::build`] (which runs the greedy
/// sampling), then [`DrivenRom::evaluate`] at any in-band frequency.
pub struct DrivenRom<'a> {
    op: &'a DrivenOperator,
    /// Interior dimension `n`.
    n: usize,
    /// Full edge count (for scattering the reconstruction).
    n_edges: usize,
    /// Interior → full edge-index map.
    interior_to_full: Vec<usize>,
    /// Orthonormal basis columns `v_j ∈ ℂⁿ`.
    basis: Vec<Vec<c64>>,
    /// Cached products `K·v_j`, `M·v_j`, `C·v_j`, `S_p·v_j` (residual
    /// indicator ingredients).
    kv: Vec<Vec<c64>>,
    mv: Vec<Vec<c64>>,
    cv: Option<Vec<Vec<c64>>>,
    spv: Vec<Vec<Vec<c64>>>,
    /// Cached products `S_Γ·v_j` per impedance surface.
    sgv: Vec<Vec<Vec<c64>>>,
    /// Projected reduced matrices (row-major rows of length k) — stored
    /// explicitly so `∂A_r/∂ω` is analytically available (see module
    /// docs, differentiability hook).
    k_r: Vec<Vec<c64>>,
    m_r: Vec<Vec<c64>>,
    c_r: Option<Vec<Vec<c64>>>,
    s_r: Vec<Vec<Vec<c64>>>,
    /// Projected impedance-surface masses `VᴴS_ΓV`.
    sg_r: Vec<Vec<Vec<c64>>>,
    /// `1/Z_p` per port (the iω-linear port-admittance coefficients).
    port_inv_z: Vec<f64>,
    /// Impedance surfaces: interior `(row, col, value)` triplets of each
    /// real `S_Γ` and its model (coefficient `iω/Z_s(ω)` re-evaluated per
    /// ω, exactly as `assemble_a_at` does).
    surf_triplets: Vec<Vec<(usize, usize, f64)>>,
    surf_models: Vec<SurfaceImpedanceModel>,
    /// The drives (right-hand sides).
    drives: Vec<Drive>,
    /// Wave (modal) channels; `None` for a lumped-only ROM.
    modal: Option<ModalData<'a>>,
    /// Per drive: fixed direction `b(ω) = c_d(ω) b̂` (interior space), its
    /// projection `b̂_r = Vᴴ b̂` and its norm.
    b_hat: Vec<Vec<c64>>,
    b_hat_r: Vec<Vec<c64>>,
    b_hat_norm: Vec<f64>,
    /// Greedy diagnostics.
    snapshot_omegas: Vec<f64>,
    converged: bool,
    worst_residual: f64,
}

impl<'a> DrivenRom<'a> {
    /// Run the greedy PROM construction over the candidate grid `omegas`
    /// (which is also the evaluation grid of [`rom_frequency_sweep`]) for
    /// the operator's baked drive ([`RomDrive::Baked`]).
    ///
    /// Seeds: band endpoints + the grid point nearest the band midpoint.
    /// Each greedy step spends one full-order solve
    /// ([`DrivenOperator::factor_at`] + back-solve, bit-identical to the
    /// dense sweep's per-ω solve) at the worst-indicator frequency. See
    /// the module docs for the determinism / tie-break contract.
    ///
    /// # Errors
    ///
    /// [`RomError::InvalidParameter`] for an empty grid,
    /// non-finite/non-positive frequencies, a zero snapshot budget, or a
    /// non-finite tolerance; any [`DrivenError`] from the snapshot solves
    /// or an impedance surface's coefficient.
    pub fn build(
        op: &'a DrivenOperator,
        omegas: &[f64],
        settings: &RomSettings,
    ) -> Result<Self, RomError> {
        Self::build_with(op, omegas, settings, RomDrive::Baked, &mut |_| {})
    }

    /// [`DrivenRom::build`] with an explicit [`RomDrive`] and a snapshot
    /// observer: `on_snapshot(ω)` is called after each full-order
    /// snapshot solve completes, in greedy selection order (progress
    /// reporting; issue #708).
    ///
    /// # Errors
    ///
    /// As [`DrivenRom::build`]; additionally [`RomError::InvalidParameter`]
    /// for [`RomDrive::PerPort`] on an operator without ports or with a
    /// port whose `v_inc` is zero (its excitation would be identically
    /// zero).
    pub fn build_with(
        op: &'a DrivenOperator,
        omegas: &[f64],
        settings: &RomSettings,
        drive: RomDrive,
        on_snapshot: &mut dyn FnMut(f64),
    ) -> Result<Self, RomError> {
        validate_request(omegas, settings)?;
        let drives: Vec<Drive> = match drive {
            RomDrive::Baked => vec![Drive::Baked],
            RomDrive::PerPort => {
                if op.n_ports() == 0 {
                    return Err(RomError::InvalidParameter(
                        "RomDrive::PerPort needs at least one lumped port".into(),
                    ));
                }
                if let Some(j) = (0..op.n_ports()).find(|&j| op.port_v_inc(j) == c64::new(0.0, 0.0))
                {
                    return Err(RomError::InvalidParameter(format!(
                        "RomDrive::PerPort: port {j} has v_inc = 0 (no excitation)"
                    )));
                }
                (0..op.n_ports()).map(Drive::Lumped).collect()
            }
        };
        let mut rom = Self::new_base(op, drives, None);
        rom.run_greedy(omegas, settings, on_snapshot)?;
        Ok(rom)
    }

    /// Build a PROM for the **mixed lumped + wave port** S-parameter
    /// excitations of `op` plus the wave ports `wave` (issue #774) — the
    /// reduced-order analog of
    /// [`crate::driven::ports::solve_mixed_port_sweep_with_mode`] (and,
    /// with no lumped ports, of the pure-wave sweep).
    ///
    /// `op` must be assembled with the lumped ports (in S-matrix order),
    /// the walls and a **zero** volume source, exactly as the dense mixed
    /// sweep assembles its base operator; `mesh` / `bcs` are the ones it
    /// was assembled on. There is one drive per channel (`N_l + Σ_p K_p`,
    /// lumped first, then wave port-major / mode-minor); each snapshot is
    /// one factorization of `A_base(ω)` plus the dense sweep's rank-`N_w`
    /// SMW post-step, and only the excitation solutions join the basis.
    /// Each wave channel's modal term `j·y_q(ω)·f_q f_qᵀ` is a rank-1
    /// family projected once (see the module docs). Read the S-matrix
    /// with [`DrivenRom::evaluate_scattering`].
    ///
    /// # Errors
    ///
    /// As [`DrivenRom::build`]; [`RomError::InvalidParameter`] without
    /// any port; [`DrivenError::InvalidPort`] (wrapped) for a lumped port
    /// with `v_inc = 0` or an invalid wave port / mode — the dense mixed
    /// sweep's errors; any [`DrivenError`] from the snapshot solves.
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_wave_ports(
        op: &'a DrivenOperator,
        mesh: &TetMesh,
        bcs: &DrivenBcs<'_>,
        wave: &'a [WavePort],
        omegas: &[f64],
        settings: &RomSettings,
        on_snapshot: &mut dyn FnMut(f64),
    ) -> Result<Self, RomError> {
        Self::build_modal(
            op,
            mesh,
            bcs,
            std::borrow::Cow::Borrowed(wave),
            omegas,
            settings,
            on_snapshot,
        )
    }

    /// [`DrivenRom::build_with_wave_ports`] over [`WavePortSpec`] ports
    /// (issue #804). Geometric ports are projected exactly as there.
    ///
    /// # Errors
    ///
    /// [`RomError::InvalidParameter`] for **any hybrid port**: its modal
    /// flux `f̂(ω)` and `β(ω)` come from a per-frequency re-solve of an
    /// inhomogeneous cross-section, so the port operator is not affine in ω
    /// and cannot be projected once (the PROM never silently freezes the
    /// mode); otherwise as [`DrivenRom::build_with_wave_ports`].
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_wave_port_specs(
        op: &'a DrivenOperator,
        mesh: &TetMesh,
        bcs: &DrivenBcs<'_>,
        wave: &[WavePortSpec],
        omegas: &[f64],
        settings: &RomSettings,
        on_snapshot: &mut dyn FnMut(f64),
    ) -> Result<Self, RomError> {
        let mut owned = Vec::with_capacity(wave.len());
        for (i, spec) in wave.iter().enumerate() {
            match spec {
                WavePortSpec::Geometric(p) => owned.push(p.clone()),
                WavePortSpec::Hybrid(_) => {
                    return Err(RomError::InvalidParameter(format!(
                        "wave port {i} is a hybrid (inhomogeneous cross-section) port: its mode \
                         shape f̂(ω) and β(ω) are re-solved per frequency and are not affine in \
                         ω, so the adaptive PROM cannot project them once; use the dense sweep \
                         (solve_wave_port_spec_sweep_with_mode / \
                         solve_mixed_port_spec_sweep_with_mode) for hybrid ports"
                    )));
                }
            }
        }
        Self::build_modal(
            op,
            mesh,
            bcs,
            std::borrow::Cow::Owned(owned),
            omegas,
            settings,
            on_snapshot,
        )
    }

    fn build_modal(
        op: &'a DrivenOperator,
        mesh: &TetMesh,
        bcs: &DrivenBcs<'_>,
        wave: std::borrow::Cow<'a, [WavePort]>,
        omegas: &[f64],
        settings: &RomSettings,
        on_snapshot: &mut dyn FnMut(f64),
    ) -> Result<Self, RomError> {
        validate_request(omegas, settings)?;
        let n_lumped = op.n_ports();
        if n_lumped == 0 && wave.is_empty() {
            return Err(RomError::InvalidParameter(
                "a mixed-port PROM needs at least one lumped or wave port".into(),
            ));
        }
        for index in 0..n_lumped {
            if op.port_v_inc(index) == c64::new(0.0, 0.0) {
                return Err(DrivenError::InvalidPort {
                    index,
                    reason: "every lumped port needs a non-zero v_inc to serve as an \
                             S-parameter excitation"
                        .to_string(),
                }
                .into());
            }
        }
        // Issue #884 (Epic #836 Phase 3a): the modal channels live on the
        // operator's own space — the p=1 edge table (the historical path,
        // verbatim) or the p=2 trace kernel on the space rebuilt from the mesh
        // (deterministic and connectivity-derived, so it is the space the
        // operator was assembled on whenever the DOF counts agree).
        let channels = match op.order() {
            crate::elements::ElementOrder::P1 => {
                let edges = mesh.edges();
                modal_channels(mesh, n_lumped, &wave, &edges)?
            }
            order => {
                let space = crate::assembly::hcurl_space::HcurlSpace::build(mesh, order);
                if space.n_dofs() != op.n_dofs() {
                    return Err(DrivenError::SpaceMeshMismatch {
                        space_nodes: space.n_nodes(),
                        space_tets: space.n_tets(),
                        mesh_nodes: mesh.n_nodes(),
                        mesh_tets: mesh.n_tets(),
                    }
                    .into());
                }
                crate::driven::ports::wave_p2::modal_channels_on_space(
                    &space, mesh, n_lumped, &wave,
                )?
            }
        };
        let fluxes_int = interior_fluxes(&channels, bcs.pec_interior_mask);
        let drives: Vec<Drive> = (0..n_lumped)
            .map(Drive::Lumped)
            .chain((0..channels.len()).map(Drive::Wave))
            .collect();
        let port_mode_counts = wave.iter().map(|p| p.modes.len()).collect();
        let modal = ModalData {
            wave,
            u: vec![Vec::new(); channels.len()],
            channels,
            fluxes_int,
            pec_mask: bcs.pec_interior_mask.to_vec(),
            port_mode_counts,
        };
        let mut rom = Self::new_base(op, drives, Some(modal));
        rom.run_greedy(omegas, settings, on_snapshot)?;
        Ok(rom)
    }

    /// An empty (no basis) ROM for `drives`: the fixed drive directions
    /// and the per-family storage.
    fn new_base(op: &'a DrivenOperator, drives: Vec<Drive>, modal: Option<ModalData<'a>>) -> Self {
        let n = op.n_interior();
        let n_edges = op.rhs_re().len();
        let interior_to_full = op.interior_to_full();

        // --- Fixed drive directions b̂, interior-filtered ------------------
        // Lumped / baked drives (b(ω) = ω·b̂) mirror `assemble_b_at`
        // exactly. Volume moments: b = iω(re + i·im) = ω(−im + i·re) ⇒
        // b̂ = −im + i·re. A wave drive's direction is its interior flux
        // f_q (b(ω) = 2j·y_q(ω)·a_inc,q · f_q).
        let volume: Vec<c64> = op
            .rhs_re()
            .iter()
            .zip(op.rhs_im().iter())
            .map(|(&re, &im)| c64::new(-im, re))
            .collect();
        let mut port_inv_z = Vec::with_capacity(op.n_ports());
        for p in 0..op.n_ports() {
            port_inv_z.push(1.0 / op.port_transient_data(p).z_s);
        }
        let b_hat: Vec<Vec<c64>> = drives
            .iter()
            .map(|&drive| {
                let excited = match drive {
                    Drive::Baked => None,
                    Drive::Lumped(j) => Some(j),
                    Drive::Wave(q) => {
                        return modal
                            .as_ref()
                            .expect("wave drive needs modal data")
                            .fluxes_int[q]
                            .clone();
                    }
                };
                let mut b_full = volume.clone();
                // Matched-source port drive: b += (2iω/Z_p)(V_inc/ℓ) f ⇒
                // b̂ += (2i/Z_p)(V_inc/ℓ) f — restricted to the excited port.
                for p in 0..op.n_ports() {
                    if excited.is_some_and(|j| j != p) {
                        continue;
                    }
                    let v_inc = op.port_v_inc(p);
                    if v_inc == c64::new(0.0, 0.0) {
                        continue;
                    }
                    let data = op.port_transient_data(p);
                    let e_inc = v_inc * (1.0 / data.length);
                    let drive = c64::new(0.0, 2.0 / data.z_s) * e_inc;
                    for (b, &f) in b_full.iter_mut().zip(data.flux.iter()) {
                        *b += drive * f;
                    }
                }
                interior_to_full.iter().map(|&j| b_full[j]).collect()
            })
            .collect();
        let b_hat_norm: Vec<f64> = b_hat.iter().map(|b| vec_norm(b)).collect();

        let (surf_triplets, surf_models): (Vec<_>, Vec<_>) = (0..op.n_surfaces())
            .map(|i| op.surface_mass_triplets(i))
            .unzip();

        let has_c = op.c_vals().is_some();
        let n_drives = drives.len();
        Self {
            op,
            n,
            n_edges,
            interior_to_full,
            basis: Vec::new(),
            kv: Vec::new(),
            mv: Vec::new(),
            cv: has_c.then(Vec::new),
            spv: vec![Vec::new(); op.n_ports()],
            sgv: vec![Vec::new(); surf_models.len()],
            k_r: Vec::new(),
            m_r: Vec::new(),
            c_r: has_c.then(Vec::new),
            s_r: vec![Vec::new(); op.n_ports()],
            sg_r: vec![Vec::new(); surf_models.len()],
            port_inv_z,
            surf_triplets,
            surf_models,
            drives,
            modal,
            b_hat,
            b_hat_r: vec![Vec::new(); n_drives],
            b_hat_norm,
            snapshot_omegas: Vec::new(),
            converged: false,
            worst_residual: f64::INFINITY,
        }
    }

    /// The greedy snapshot selection (seeds, then worst-indicator picks)
    /// over the candidate grid; see the module docs.
    fn run_greedy(
        &mut self,
        omegas: &[f64],
        settings: &RomSettings,
        on_snapshot: &mut dyn FnMut(f64),
    ) -> Result<(), RomError> {
        let rom = self;
        // Zero drive: every solution is identically zero (matching the
        // dense sweep's zero-RHS semantics); nothing to sample.
        if rom.b_hat_norm.iter().all(|&b| b == 0.0) {
            rom.converged = true;
            rom.worst_residual = 0.0;
            return Ok(());
        }

        // Candidate order: ascending ω (stable in original index for exact
        // duplicates) — the iteration order that realizes the documented
        // lowest-frequency tie-break.
        let mut order: Vec<usize> = (0..omegas.len()).collect();
        order.sort_by(|&i, &j| omegas[i].partial_cmp(&omegas[j]).unwrap().then(i.cmp(&j)));
        let lo_idx = order[0];
        let hi_idx = *order.last().unwrap();

        // Seeds: endpoints + grid point nearest the band midpoint (strict
        // `<` keeps the lowest such frequency on ties).
        let midpoint = 0.5 * (omegas[lo_idx] + omegas[hi_idx]);
        let mut mid_idx = lo_idx;
        let mut mid_dist = f64::INFINITY;
        for &i in &order {
            let d = (omegas[i] - midpoint).abs();
            if d < mid_dist {
                mid_dist = d;
                mid_idx = i;
            }
        }
        let mut seeds = vec![lo_idx, mid_idx, hi_idx];
        seeds.sort_by(|&i, &j| omegas[i].partial_cmp(&omegas[j]).unwrap());
        seeds.dedup();
        // Also drop distinct indices carrying duplicate ω values.
        seeds.dedup_by(|a, b| omegas[*a] == omegas[*b]);

        let mut used = vec![false; omegas.len()];
        for &idx in &seeds {
            if rom.snapshot_omegas.len() >= settings.max_snapshots {
                break;
            }
            used[idx] = true;
            rom.add_snapshot(omegas[idx])?;
            on_snapshot(omegas[idx]);
        }

        // --- Greedy refinement --------------------------------------------
        loop {
            // Residual indicator over the whole grid (snapshot points
            // included — they are ~roundoff and part of the honest "worst
            // over the band" figure).
            let mut worst = 0.0_f64;
            let mut best_idx: Option<usize> = None;
            let mut best_res = 0.0_f64;
            for &i in &order {
                let eta = rom.indicator_at(omegas[i]);
                worst = worst.max(eta);
                // Strict `>` + ascending-ω iteration = lowest-ω tie-break.
                if !used[i] && eta > best_res {
                    best_res = eta;
                    best_idx = Some(i);
                }
            }
            rom.worst_residual = worst;
            if worst <= settings.tolerance {
                rom.converged = true;
                break;
            }
            if rom.snapshot_omegas.len() >= settings.max_snapshots || rom.basis.len() >= rom.n {
                break;
            }
            let Some(idx) = best_idx else {
                // Candidate grid exhausted without reaching tolerance.
                break;
            };
            used[idx] = true;
            let grew = rom.add_snapshot(omegas[idx])?;
            on_snapshot(omegas[idx]);
            if !grew {
                // Snapshot linearly dependent on the current basis: the
                // candidate is consumed (never re-picked) but the ROM did
                // not change; keep going with the remaining candidates.
                continue;
            }
        }
        Ok(())
    }

    /// Snapshot frequencies in greedy selection order (one full-order
    /// factorization each).
    pub fn snapshot_omegas(&self) -> &[f64] {
        &self.snapshot_omegas
    }

    /// Reduced dimension `k` (≤ number of snapshots × drives; smaller
    /// when a snapshot was linearly dependent).
    pub fn reduced_order(&self) -> usize {
        self.basis.len()
    }

    /// Whether the greedy loop reached [`RomSettings::tolerance`].
    pub fn converged(&self) -> bool {
        self.converged
    }

    /// Worst residual indicator over the candidate grid at termination.
    pub fn worst_residual(&self) -> f64 {
        self.worst_residual
    }

    /// Evaluate the PROM at one frequency: dense `k×k` solve, residual
    /// indicator against the **full-order** operator, and port readouts
    /// on the reconstruction `x = V x_r` (same flux functional and
    /// Thevenin relation as the dense sweep).
    ///
    /// `omega` need not be a grid point — the ROM is a continuous-in-ω
    /// surrogate; the indicator stays honest off-grid too.
    ///
    /// For a [`RomDrive::PerPort`] ROM this returns the **first** drive
    /// (port 0 excited) only; use [`DrivenRom::evaluate_excitations`].
    ///
    /// # Errors
    ///
    /// [`RomError::ReducedSolveSingular`] if the reduced system has no
    /// solution at this frequency; [`DrivenError`] from an impedance
    /// surface's coefficient.
    pub fn evaluate(&self, omega: f64) -> Result<RomSweepPoint, RomError> {
        let mut p = self.evaluate_excitations(omega)?;
        Ok(RomSweepPoint {
            omega,
            residual_indicator: p.residual_indicator,
            ports: p.excitations.swap_remove(0),
        })
    }

    /// The reconstructed full-length DOF vector `x ≈ V x_r` of the
    /// **first** drive at `omega`, plus the residual indicator — a
    /// read-only accessor for field-level observables (issue #838: the
    /// port-free p=2 self-oracle). The layout is the operator's space
    /// ([`DrivenOperator::n_dofs`], the edge vector at p=1). It reuses the
    /// reduced solve and indicator of [`DrivenRom::evaluate`] unchanged.
    ///
    /// # Errors
    ///
    /// As [`DrivenRom::evaluate`]; [`RomError::InvalidParameter`] for a
    /// wave-port PROM (use [`DrivenRom::evaluate_scattering`]).
    pub fn evaluate_field(&self, omega: f64) -> Result<(Vec<c64>, f64), RomError> {
        if self.modal.is_some() {
            return Err(RomError::InvalidParameter(
                "a wave-port PROM has no single-drive field readout; use \
                 DrivenRom::evaluate_scattering"
                    .into(),
            ));
        }
        let k = self.basis.len();
        let coeffs = self.surface_coefficients(omega)?;
        let x_rs = if k == 0 {
            vec![Vec::new(); self.drives.len()]
        } else {
            self.try_reduced_solve(omega, &coeffs, &[])
                .ok_or(RomError::ReducedSolveSingular { order: k, omega })?
        };
        let residual_indicator = self.residual_indicator(omega, &coeffs, &[], &x_rs);
        Ok((self.reconstruct_full(&x_rs[0]), residual_indicator))
    }

    /// Evaluate the PROM at one frequency for **every** drive (see
    /// [`RomDrive`]): the per-drive port readouts plus the worst residual
    /// indicator over the drives.
    ///
    /// # Errors
    ///
    /// As [`DrivenRom::evaluate`].
    pub fn evaluate_excitations(&self, omega: f64) -> Result<RomExcitationPoint, RomError> {
        if self.modal.is_some() {
            return Err(RomError::InvalidParameter(
                "a wave-port PROM has no per-port circuit readout; use \
                 DrivenRom::evaluate_scattering"
                    .into(),
            ));
        }
        let k = self.basis.len();
        let coeffs = self.surface_coefficients(omega)?;
        let x_rs = if k == 0 {
            vec![Vec::new(); self.drives.len()]
        } else {
            self.try_reduced_solve(omega, &coeffs, &[])
                .ok_or(RomError::ReducedSolveSingular { order: k, omega })?
        };
        let residual_indicator = self.residual_indicator(omega, &coeffs, &[], &x_rs);

        let mut excitations = Vec::with_capacity(self.drives.len());
        for (&drive, x_r) in self.drives.iter().zip(&x_rs) {
            let e_edges = self.reconstruct_full(x_r);
            let ports = (0..self.op.n_ports())
                .map(|p| {
                    let v = self.op.port_voltage(p, &e_edges);
                    let i = match drive {
                        Drive::Lumped(j) => {
                            // Driven only in its own excitation; elsewhere
                            // a passive termination (V_inc = 0).
                            let v_inc = if p == j {
                                self.op.port_v_inc(p)
                            } else {
                                c64::new(0.0, 0.0)
                            };
                            self.op.port_current_with_v_inc(p, v_inc, v)
                        }
                        Drive::Baked | Drive::Wave(_) => self.op.port_current(p, v),
                    };
                    PortCircuit { v, i, z: v / i }
                })
                .collect();
            excitations.push(ports);
        }
        Ok(RomExcitationPoint {
            omega,
            residual_indicator,
            excitations,
        })
    }

    /// Evaluate a wave-port / mixed-port PROM
    /// ([`DrivenRom::build_with_wave_ports`]) at one frequency: one dense
    /// `k×k` solve per channel, the worst full-order residual indicator
    /// (modal terms included) and the power-wave S-matrix read out on the
    /// reconstruction `x = V x_r` with the dense mixed sweep's own
    /// excitation / readout helpers (lumped `ã = V_inc/√R`, wave
    /// `ã = a_inc·√y/√ω`). `β` comes from the same call as the dense
    /// sweep.
    ///
    /// At a frequency exactly at a lossless cutoff (`y = 0`) the wave
    /// drive vanishes and the S column is non-finite — as in the dense
    /// sweep; callers fall back to it there.
    ///
    /// # Errors
    ///
    /// [`RomError::InvalidParameter`] for a ROM built without
    /// [`DrivenRom::build_with_wave_ports`]; otherwise as
    /// [`DrivenRom::evaluate`].
    pub fn evaluate_scattering(&self, omega: f64) -> Result<RomScatteringPoint, RomError> {
        let Some(modal) = &self.modal else {
            return Err(RomError::InvalidParameter(
                "evaluate_scattering needs a PROM built with DrivenRom::build_with_wave_ports"
                    .into(),
            ));
        };
        let k = self.basis.len();
        let coeffs = self.surface_coefficients(omega)?;
        let (betas, ys) = channel_admittances(&modal.wave, &modal.channels, omega);
        let x_rs = if k == 0 {
            vec![Vec::new(); self.drives.len()]
        } else {
            self.try_reduced_solve(omega, &coeffs, &ys)
                .ok_or(RomError::ReducedSolveSingular { order: k, omega })?
        };
        let residual_indicator = self.residual_indicator(omega, &coeffs, &ys, &x_rs);

        let n_lumped = self.op.n_ports();
        let n_ports = self.drives.len();
        let weights = PowerWeights::new(self.op, n_lumped, &ys, omega);
        let mut s = vec![c64::new(0.0, 0.0); n_ports * n_ports];
        for (j, x_r) in x_rs.iter().enumerate() {
            let x_int = self.reconstruct_interior(x_r);
            let a_tilde = incident_wave(self.op, j, n_lumped, &modal.channels, &weights);
            s_column(
                self.op,
                &modal.pec_mask,
                self.n_edges,
                &modal.channels,
                &weights,
                j,
                a_tilde,
                &x_int,
                &mut s,
            );
        }
        Ok(RomScatteringPoint {
            omega,
            residual_indicator,
            s,
            beta: betas,
            n_ports,
            n_lumped,
            port_mode_counts: modal.port_mode_counts.clone(),
        })
    }

    /// `x = V x_r` (interior).
    fn reconstruct_interior(&self, x_r: &[c64]) -> Vec<c64> {
        let mut x_int = vec![c64::new(0.0, 0.0); self.n];
        for (j, v) in self.basis.iter().enumerate() {
            let xj = x_r[j];
            for (xi, &vi) in x_int.iter_mut().zip(v.iter()) {
                *xi += vi * xj;
            }
        }
        x_int
    }

    /// `x = V x_r` scattered to the full edge vector.
    fn reconstruct_full(&self, x_r: &[c64]) -> Vec<c64> {
        let x_int = self.reconstruct_interior(x_r);
        let mut e_edges = vec![c64::new(0.0, 0.0); self.n_edges];
        for (i, &full) in self.interior_to_full.iter().enumerate() {
            e_edges[full] = x_int[i];
        }
        e_edges
    }

    /// One full-order snapshot at `omega` — **one** factorization, one
    /// back-solve per drive (plus, with wave ports, the dense mixed
    /// sweep's rank-`N_w` SMW post-step) — each solution
    /// MGS-orthonormalized into the basis. Returns `Ok(false)` (without
    /// growing the ROM) when every drive's snapshot is numerically
    /// dependent on the current basis.
    fn add_snapshot(&mut self, omega: f64) -> Result<bool, RomError> {
        self.snapshot_omegas.push(omega);
        let factor = self.op.factor_at(omega)?;
        let mut grew = false;
        if let Some(modal) = &self.modal {
            // SMW-corrected excitation solves, exactly as
            // `solve_mixed_port_sweep_with_mode` forms them.
            let (_, ys) = channel_admittances(&modal.wave, &modal.channels, omega);
            let mut back_solve = |b: &[c64], x: &mut [c64]| factor.back_solve(b, x).map(|()| 0);
            let mut iters = Vec::new();
            let smw = ModalSmw::prepare(
                &modal.fluxes_int,
                &ys,
                self.n,
                omega,
                &mut back_solve,
                &mut iters,
            )?;
            let n_lumped = self.op.n_ports();
            let mut sols = Vec::with_capacity(self.drives.len());
            for j in 0..self.drives.len() {
                let b = excitation_rhs(
                    self.op,
                    omega,
                    j,
                    n_lumped,
                    &modal.channels,
                    &modal.fluxes_int,
                    &ys,
                );
                sols.push(smw.solve(&modal.fluxes_int, &b, &mut back_solve, &mut iters)?);
            }
            for w in sols {
                grew |= self.add_column(w);
            }
            return Ok(grew);
        }
        let drives = self.drives.clone();
        for drive in drives {
            let sol = match drive {
                Drive::Baked => factor.solve()?,
                Drive::Lumped(j) => factor.solve_excited(j)?,
                Drive::Wave(_) => unreachable!("wave drives carry modal data"),
            };
            let w: Vec<c64> = self
                .interior_to_full
                .iter()
                .map(|&j| sol.e_edges[j])
                .collect();
            grew |= self.add_column(w);
        }
        Ok(grew)
    }

    /// MGS-orthonormalize `w` against the basis and, if independent,
    /// append it (products, reduced matrices, projected drives). Returns
    /// whether the basis grew.
    fn add_column(&mut self, mut w: Vec<c64>) -> bool {
        // Modified Gram–Schmidt with one re-orthogonalization pass.
        let norm0 = vec_norm(&w);
        for _pass in 0..2 {
            for v in &self.basis {
                let h = dot_h(v, &w);
                for (wi, &vi) in w.iter_mut().zip(v.iter()) {
                    *wi -= vi * h;
                }
            }
        }
        let norm = vec_norm(&w);
        if norm0 == 0.0 || norm <= 1e-12 * norm0 {
            return false;
        }
        let inv = 1.0 / norm;
        for wi in w.iter_mut() {
            *wi *= inv;
        }

        // Matrix–basis products for the new column.
        let kw = triplet_matvec_c(self.op.rows(), self.op.cols(), self.op.k_vals(), &w, self.n);
        let mw = triplet_matvec_c(self.op.rows(), self.op.cols(), self.op.m_vals(), &w, self.n);
        let cw = self
            .op
            .c_vals()
            .map(|c| triplet_matvec_r(self.op.rows(), self.op.cols(), c, &w, self.n));
        let spw: Vec<Vec<c64>> = (0..self.op.n_ports())
            .map(|p| {
                let data = self.op.port_transient_data(p);
                triplet_list_matvec(data.mass_triplets, &w, self.n)
            })
            .collect();
        let sgw: Vec<Vec<c64>> = self
            .surf_triplets
            .iter()
            .map(|t| triplet_list_matvec(t, &w, self.n))
            .collect();

        // Grow the reduced matrices: new column (Vᴴ·(X w)), new row
        // (wᴴ·(X v_j)), corner (wᴴ·(X w)).
        grow_reduced(&mut self.k_r, &self.basis, &self.kv, &w, &kw);
        grow_reduced(&mut self.m_r, &self.basis, &self.mv, &w, &mw);
        if let (Some(c_r), Some(cv), Some(cw)) = (self.c_r.as_mut(), self.cv.as_ref(), cw.as_ref())
        {
            grow_reduced(c_r, &self.basis, cv, &w, cw);
        }
        for (p, spw_p) in spw.iter().enumerate() {
            grow_reduced(&mut self.s_r[p], &self.basis, &self.spv[p], &w, spw_p);
        }
        for (g, sgw_g) in sgw.iter().enumerate() {
            grow_reduced(&mut self.sg_r[g], &self.basis, &self.sgv[g], &w, sgw_g);
        }
        for (b_r, b) in self.b_hat_r.iter_mut().zip(&self.b_hat) {
            b_r.push(dot_h(&w, b));
        }
        // Rank-1 modal families: u_q gains f_qᵀ w (f real — the
        // unconjugated pairing; the family projects as conj(u) uᵀ).
        if let Some(modal) = self.modal.as_mut() {
            for (u, f) in modal.u.iter_mut().zip(&modal.fluxes_int) {
                u.push(dot_t(f, &w));
            }
        }

        // Commit the column.
        self.kv.push(kw);
        self.mv.push(mw);
        if let (Some(cv), Some(cw)) = (self.cv.as_mut(), cw) {
            cv.push(cw);
        }
        for (p, spw_p) in spw.into_iter().enumerate() {
            self.spv[p].push(spw_p);
        }
        for (g, sgw_g) in sgw.into_iter().enumerate() {
            self.sgv[g].push(sgw_g);
        }
        self.basis.push(w);
        true
    }

    /// The impedance surfaces' weak coefficients `iω/Z_s(ω)` at `omega`.
    fn surface_coefficients(&self, omega: f64) -> Result<Vec<c64>, DrivenError> {
        self.surf_models
            .iter()
            .map(|m| m.weak_coefficient(omega))
            .collect()
    }

    /// The wave channels' admittance factors `y_q(ω)` (empty for a
    /// lumped-only ROM).
    fn admittances(&self, omega: f64) -> Vec<c64> {
        self.modal.as_ref().map_or_else(Vec::new, |m| {
            channel_admittances(&m.wave, &m.channels, omega).1
        })
    }

    /// The scalar drive coefficient `c_d(ω)` of `b_d(ω) = c_d(ω)·b̂_d`:
    /// `ω` for a baked / lumped drive, `2j·y_q·a_inc,q` for wave channel
    /// `q` (the dense sweep's `coeff`).
    fn drive_coefficient(&self, drive: Drive, omega: f64, ys: &[c64]) -> c64 {
        match drive {
            Drive::Baked | Drive::Lumped(_) => c64::new(omega, 0.0),
            Drive::Wave(q) => {
                let a_inc = self.modal.as_ref().expect("wave drive").channels[q].a_inc;
                c64::new(0.0, 2.0) * ys[q] * a_inc
            }
        }
    }

    /// The greedy indicator at `omega`: worst over the drives, `∞` when
    /// the reduced system (or a surface coefficient) is singular there.
    fn indicator_at(&self, omega: f64) -> f64 {
        let Ok(coeffs) = self.surface_coefficients(omega) else {
            return f64::INFINITY;
        };
        let ys = self.admittances(omega);
        match self.try_reduced_solve(omega, &coeffs, &ys) {
            Some(x_rs) => self.residual_indicator(omega, &coeffs, &ys, &x_rs),
            None => f64::INFINITY,
        }
    }

    /// Assemble the reduced `A_r(ω)` once and solve `A_r(ω) x_r = c_d(ω)
    /// b̂_{r,d}` for every drive. `ys` are the wave channels' `y_q(ω)`
    /// (empty without wave ports). `None` when the dense LU hits a
    /// zero/non-finite pivot.
    fn try_reduced_solve(&self, omega: f64, coeffs: &[c64], ys: &[c64]) -> Option<Vec<Vec<c64>>> {
        let k = self.basis.len();
        if k == 0 {
            return Some(vec![Vec::new(); self.drives.len()]);
        }
        let omega2 = omega * omega;
        let i_omega = c64::new(0.0, omega);
        // Modal families: + j·y_q · conj(u_q) u_qᵀ.
        let modal_terms: Vec<(c64, &[c64])> = match &self.modal {
            Some(m) => ys
                .iter()
                .zip(&m.u)
                .map(|(&y, u)| (c64::new(0.0, 1.0) * y, u.as_slice()))
                .collect(),
            None => Vec::new(),
        };
        let mut a = vec![c64::new(0.0, 0.0); k * k];
        for r in 0..k {
            for c in 0..k {
                let mut v = self.k_r[r][c] - self.m_r[r][c] * omega2;
                if let Some(c_r) = &self.c_r {
                    v += i_omega * c_r[r][c];
                }
                for (p, s_r) in self.s_r.iter().enumerate() {
                    v += i_omega * self.port_inv_z[p] * s_r[r][c];
                }
                for (coeff, sg_r) in coeffs.iter().zip(&self.sg_r) {
                    v += *coeff * sg_r[r][c];
                }
                for &(jy, u) in &modal_terms {
                    v += jy * (u[r].conj() * u[c]);
                }
                a[r * k + c] = v;
            }
        }
        self.b_hat_r
            .iter()
            .zip(&self.drives)
            .map(|(b_r, &drive)| {
                let b: Vec<c64> = match drive {
                    Drive::Baked | Drive::Lumped(_) => b_r.iter().map(|&x| x * omega).collect(),
                    Drive::Wave(_) => {
                        let cd = self.drive_coefficient(drive, omega, ys);
                        b_r.iter().map(|&x| x * cd).collect()
                    }
                };
                solve_dense_lu(a.clone(), b, k)
            })
            .collect()
    }

    /// `η(ω) = max_d ‖A(ω)Vx_{r,d} − c_d b̂_d‖ / (|c_d|·‖b̂_d‖)` via the
    /// cached matrix–basis products — `O(nk)` per drive, no sparse
    /// assembly. With wave ports `A(ω)` includes the modal terms
    /// `Σ_q j·y_q f_q f_qᵀ`, applied as `f_q·(u_qᵀ x_r)` (`O(n)` each):
    /// the same full-order residual the dense mixed / wave sweeps report.
    fn residual_indicator(&self, omega: f64, coeffs: &[c64], ys: &[c64], x_rs: &[Vec<c64>]) -> f64 {
        let omega2 = omega * omega;
        let mut worst = 0.0_f64;
        for (((b_hat, &b_norm), x_r), &drive) in self
            .b_hat
            .iter()
            .zip(&self.b_hat_norm)
            .zip(x_rs)
            .zip(&self.drives)
        {
            let cd = self.drive_coefficient(drive, omega, ys);
            let mut r: Vec<c64> = match drive {
                Drive::Baked | Drive::Lumped(_) => b_hat.iter().map(|&b| b * (-omega)).collect(),
                Drive::Wave(_) => b_hat.iter().map(|&b| -(b * cd)).collect(),
            };
            for (j, &xj) in x_r.iter().enumerate() {
                let sk = xj;
                let sm = xj * (-omega2);
                let sc = xj * c64::new(0.0, omega);
                for ((ri, &kvi), &mvi) in r.iter_mut().zip(self.kv[j].iter()).zip(self.mv[j].iter())
                {
                    *ri += kvi * sk + mvi * sm;
                }
                if let Some(cv) = &self.cv {
                    for (ri, &cvi) in r.iter_mut().zip(cv[j].iter()) {
                        *ri += cvi * sc;
                    }
                }
                for (p, spv) in self.spv.iter().enumerate() {
                    let sp = sc * self.port_inv_z[p];
                    for (ri, &si) in r.iter_mut().zip(spv[j].iter()) {
                        *ri += si * sp;
                    }
                }
                for (coeff, sgv) in coeffs.iter().zip(&self.sgv) {
                    let sg = xj * *coeff;
                    for (ri, &si) in r.iter_mut().zip(sgv[j].iter()) {
                        *ri += si * sg;
                    }
                }
            }
            if let Some(modal) = &self.modal {
                for ((&y, u), f) in ys.iter().zip(&modal.u).zip(&modal.fluxes_int) {
                    let scaled = c64::new(0.0, 1.0) * y * dot_t(u, x_r);
                    for (ri, &fi) in r.iter_mut().zip(f.iter()) {
                        *ri += fi * scaled;
                    }
                }
            }
            let num = vec_norm(&r);
            let den = cd.norm() * b_norm;
            worst = worst.max(if den == 0.0 { num } else { num / den });
        }
        worst
    }
}

/// The structural checks shared by every PROM build: a non-empty grid of
/// finite positive frequencies, a non-zero snapshot budget and a finite
/// non-negative tolerance.
fn validate_request(omegas: &[f64], settings: &RomSettings) -> Result<(), RomError> {
    if omegas.is_empty() {
        return Err(RomError::InvalidParameter(
            "empty candidate frequency grid".into(),
        ));
    }
    if let Some(&bad) = omegas.iter().find(|w| !w.is_finite() || **w <= 0.0) {
        return Err(RomError::InvalidParameter(format!(
            "candidate frequency {bad} is not finite and positive"
        )));
    }
    if settings.max_snapshots == 0 {
        return Err(RomError::InvalidParameter(
            "max_snapshots must be at least 1".into(),
        ));
    }
    if !settings.tolerance.is_finite() || settings.tolerance < 0.0 {
        return Err(RomError::InvalidParameter(format!(
            "tolerance {} must be finite and non-negative",
            settings.tolerance
        )));
    }
    Ok(())
}

/// `y = S·x` from an interior `(row, col, value)` triplet list.
fn triplet_list_matvec(triplets: &[(usize, usize, f64)], x: &[c64], n: usize) -> Vec<c64> {
    let mut y = vec![c64::new(0.0, 0.0); n];
    for &(r, c, v) in triplets {
        y[r] += x[c] * v;
    }
    y
}

/// Grow a reduced matrix (rows of length `k`) to `(k+1)×(k+1)` given the
/// existing basis (`k` columns), the existing product columns `xv[j] =
/// X·v_j`, the **normalized** new basis column `w`, and its product
/// `xw = X·w`:
/// new column entries `v_iᴴ·xw`, new row entries `wᴴ·xv_j`, corner `wᴴ·xw`.
fn grow_reduced(
    reduced: &mut Vec<Vec<c64>>,
    basis: &[Vec<c64>],
    xv: &[Vec<c64>],
    w: &[c64],
    xw: &[c64],
) {
    debug_assert_eq!(reduced.len(), basis.len());
    debug_assert_eq!(xv.len(), basis.len());
    for (row, v) in reduced.iter_mut().zip(basis.iter()) {
        row.push(dot_h(v, xw));
    }
    let mut new_row: Vec<c64> = xv.iter().map(|col| dot_h(w, col)).collect();
    new_row.push(dot_h(w, xw));
    reduced.push(new_row);
}

/// Hermitian inner product `Σᵢ conj(aᵢ)·bᵢ`.
fn dot_h(a: &[c64], b: &[c64]) -> c64 {
    debug_assert_eq!(a.len(), b.len());
    let mut acc = c64::new(0.0, 0.0);
    for (&ai, &bi) in a.iter().zip(b.iter()) {
        acc += ai.conj() * bi;
    }
    acc
}

/// Euclidean norm of a complex vector.
fn vec_norm(v: &[c64]) -> f64 {
    v.iter()
        .map(|z| z.re * z.re + z.im * z.im)
        .sum::<f64>()
        .sqrt()
}

/// `y = A·x` from a complex triplet stream (duplicates sum, as in the
/// sparse assembly).
fn triplet_matvec_c(rows: &[usize], cols: &[usize], vals: &[c64], x: &[c64], n: usize) -> Vec<c64> {
    let mut y = vec![c64::new(0.0, 0.0); n];
    for ((&r, &c), &v) in rows.iter().zip(cols.iter()).zip(vals.iter()) {
        y[r] += x[c] * v;
    }
    y
}

/// `y = A·x` from a real triplet stream.
fn triplet_matvec_r(rows: &[usize], cols: &[usize], vals: &[f64], x: &[c64], n: usize) -> Vec<c64> {
    let mut y = vec![c64::new(0.0, 0.0); n];
    for ((&r, &c), &v) in rows.iter().zip(cols.iter()).zip(vals.iter()) {
        y[r] += x[c] * v;
    }
    y
}

/// Dense complex LU with partial pivoting: solve the row-major `n×n`
/// system `A x = b` in place. `None` on a zero / non-finite pivot.
fn solve_dense_lu(mut a: Vec<c64>, mut b: Vec<c64>, n: usize) -> Option<Vec<c64>> {
    debug_assert_eq!(a.len(), n * n);
    debug_assert_eq!(b.len(), n);
    for col in 0..n {
        // Partial pivot on the largest modulus in the column.
        let mut piv = col;
        let mut pmax = a[col * n + col].norm();
        for r in (col + 1)..n {
            let m = a[r * n + col].norm();
            if m > pmax {
                pmax = m;
                piv = r;
            }
        }
        if pmax == 0.0 || !pmax.is_finite() {
            return None;
        }
        if piv != col {
            for c in 0..n {
                a.swap(col * n + c, piv * n + c);
            }
            b.swap(col, piv);
        }
        let d = a[col * n + col];
        for r in (col + 1)..n {
            let f = a[r * n + col] / d;
            if f == c64::new(0.0, 0.0) {
                continue;
            }
            a[r * n + col] = c64::new(0.0, 0.0);
            for c in (col + 1)..n {
                let t = a[col * n + c] * f;
                a[r * n + c] -= t;
            }
            let t = b[col] * f;
            b[r] -= t;
        }
    }
    // Back-substitution.
    let mut x = vec![c64::new(0.0, 0.0); n];
    for col in (0..n).rev() {
        let mut acc = b[col];
        for c in (col + 1)..n {
            acc -= a[col * n + c] * x[c];
        }
        x[col] = acc / a[col * n + col];
    }
    Some(x)
}

/// Adaptive fast frequency sweep: the PROM analog of
/// [`crate::driven::extraction::driven_frequency_sweep`] — assemble the
/// ω-independent operator **once**, build a greedy Galerkin PROM over the
/// requested grid (a few full-order snapshot solves), then evaluate every
/// requested frequency through the dense `k×k` reduced system.
///
/// The dense sweep is untouched: this entry point shares
/// [`DrivenOperator::assemble`] and the per-snapshot
/// [`DrivenOperator::factor_at`] + back-solve with it bit-for-bit, and the
/// port readouts run through the same flux functional / Thevenin relation
/// on the reconstruction `x ≈ V x_r`.
///
/// Impedance `surfaces` (Leontovich / Silver-Müller / London) are
/// projected once with their scalar coefficient re-evaluated per ω (see
/// the module docs, issue #708).
///
/// # Errors
///
/// [`RomError::InvalidParameter`] / [`RomError::ReducedSolveSingular`] as
/// in [`DrivenRom::build`] / [`DrivenRom::evaluate`]; any [`DrivenError`]
/// from assembly or the snapshot solves.
#[allow(clippy::too_many_arguments)]
pub fn rom_frequency_sweep<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    source: &CurrentSource,
    settings: &RomSettings,
    device: &B::Device,
) -> Result<RomSweepReport, RomError> {
    let op = DrivenOperator::assemble::<B>(
        mesh, materials, sigma_tet, bcs, ports, surfaces, source, device,
    )?;
    let rom = DrivenRom::build(&op, omegas, settings)?;
    let points = omegas
        .iter()
        .map(|&w| rom.evaluate(w))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RomSweepReport {
        points,
        snapshot_omegas: rom.snapshot_omegas().to_vec(),
        converged: rom.converged(),
        worst_residual: rom.worst_residual(),
    })
}

/// Adaptive fast sweep for **mixed lumped + wave ports** (issue #774):
/// the PROM analog of
/// [`crate::driven::ports::solve_mixed_port_sweep_with_mode`] (direct
/// solver). Assembles the base operator once exactly as the dense mixed
/// sweep does (lumped loads + `surfaces`, zero volume source), builds a
/// [`DrivenRom::build_with_wave_ports`] PROM over `omegas`, and evaluates
/// every requested frequency with [`DrivenRom::evaluate_scattering`].
/// `lumped` may be empty (pure-wave), `wave` may be empty (then this is a
/// power-wave lumped sweep), not both.
///
/// # Errors
///
/// As [`DrivenRom::build_with_wave_ports`] /
/// [`DrivenRom::evaluate_scattering`]; any [`DrivenError`] from assembly.
#[allow(clippy::too_many_arguments)]
pub fn rom_mixed_port_sweep<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePort],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    settings: &RomSettings,
    device: &B::Device,
) -> Result<RomScatteringReport, RomError> {
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
    let rom =
        DrivenRom::build_with_wave_ports(&op, mesh, bcs, wave, omegas, settings, &mut |_| {})?;
    let points = omegas
        .iter()
        .map(|&w| rom.evaluate_scattering(w))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RomScatteringReport {
        points,
        snapshot_omegas: rom.snapshot_omegas().to_vec(),
        converged: rom.converged(),
        worst_residual: rom.worst_residual(),
        reduced_order: rom.reduced_order(),
    })
}
