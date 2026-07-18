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
//! A(ω) = K − ω²M + iω C(σ) + Σ_p (iω/Z_p) S_p        (+ Leontovich, rejected)
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
//! # Out of scope (v1) and follow-on hooks
//!
//! - **Leontovich surface impedances** carry a non-polynomial scalar
//!   coefficient `c_Γ(ω) ∝ √ω(1+i)`; although `VᴴS_ΓV` would project once
//!   with per-ω scalar evaluation, v1 rejects them
//!   ([`RomError::UnsupportedOperator`]) — the same v1 boundary the
//!   transient solver drew. Wave ports are structurally absent from
//!   [`DrivenOperator`].
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
use crate::driven::ports::LumpedPort;
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
};
use crate::mesh::TetMesh;

/// Errors from the PROM fast-sweep path.
#[derive(Debug, thiserror::Error)]
pub enum RomError {
    /// The source [`DrivenOperator`] (or sweep configuration) carries an
    /// ω-dependent term that is not polynomial in `iω` with fixed
    /// matrices — a Leontovich impedance surface in v1.
    #[error(
        "PROM sweep requires an operator polynomial in iω with fixed matrices: {reason} \
         (lumped ports + σ + PEC + fixed complex materials only; Leontovich surface \
         impedance is out of scope for v1)"
    )]
    UnsupportedOperator { reason: String },
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
    /// Projected reduced matrices (row-major rows of length k) — stored
    /// explicitly so `∂A_r/∂ω` is analytically available (see module
    /// docs, differentiability hook).
    k_r: Vec<Vec<c64>>,
    m_r: Vec<Vec<c64>>,
    c_r: Option<Vec<Vec<c64>>>,
    s_r: Vec<Vec<Vec<c64>>>,
    /// `1/Z_p` per port (the iω-linear port-admittance coefficients).
    port_inv_z: Vec<f64>,
    /// Fixed drive direction: `b(ω) = ω b̂` (interior space) and its
    /// projection `b̂_r = Vᴴ b̂`.
    b_hat: Vec<c64>,
    b_hat_r: Vec<c64>,
    b_hat_norm: f64,
    /// Greedy diagnostics.
    snapshot_omegas: Vec<f64>,
    converged: bool,
    worst_residual: f64,
}

impl<'a> DrivenRom<'a> {
    /// Run the greedy PROM construction over the candidate grid `omegas`
    /// (which is also the evaluation grid of [`rom_frequency_sweep`]).
    ///
    /// Seeds: band endpoints + the grid point nearest the band midpoint.
    /// Each greedy step spends one full-order solve
    /// ([`DrivenOperator::factor_at`] + back-solve, bit-identical to the
    /// dense sweep's per-ω solve) at the worst-indicator frequency. See
    /// the module docs for the determinism / tie-break contract.
    ///
    /// # Errors
    ///
    /// [`RomError::UnsupportedOperator`] if the operator carries
    /// Leontovich surfaces; [`RomError::InvalidParameter`] for an empty
    /// grid, non-finite/non-positive frequencies, a zero snapshot budget,
    /// or a non-finite tolerance; any [`DrivenError`] from the snapshot
    /// solves.
    pub fn build(
        op: &'a DrivenOperator,
        omegas: &[f64],
        settings: &RomSettings,
    ) -> Result<Self, RomError> {
        if op.has_surfaces() {
            return Err(RomError::UnsupportedOperator {
                reason: format!(
                    "{} Leontovich impedance surface(s) on the operator",
                    op.n_surfaces()
                ),
            });
        }
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

        let n = op.n_interior();
        let n_edges = op.rhs_re().len();
        let interior_to_full = op.interior_to_full();

        // --- Fixed drive direction b̂ (b(ω) = ω·b̂), interior-filtered ----
        // Volume moments: b = iω(re + i·im) = ω(−im + i·re) ⇒ b̂ = −im + i·re.
        let mut b_hat_full: Vec<c64> = op
            .rhs_re()
            .iter()
            .zip(op.rhs_im().iter())
            .map(|(&re, &im)| c64::new(-im, re))
            .collect();
        // Matched-source port drive: b += (2iω/Z_p)(V_inc/ℓ) f ⇒
        // b̂ += (2i/Z_p)(V_inc/ℓ) f — mirrors `assemble_b_at` exactly.
        let mut port_inv_z = Vec::with_capacity(op.n_ports());
        for p in 0..op.n_ports() {
            let data = op.port_transient_data(p);
            port_inv_z.push(1.0 / data.z_s);
            let v_inc = op.port_v_inc(p);
            if v_inc == c64::new(0.0, 0.0) {
                continue;
            }
            let e_inc = v_inc * (1.0 / data.length);
            let drive = c64::new(0.0, 2.0 / data.z_s) * e_inc;
            for (b, &f) in b_hat_full.iter_mut().zip(data.flux.iter()) {
                *b += drive * f;
            }
        }
        let b_hat: Vec<c64> = interior_to_full.iter().map(|&j| b_hat_full[j]).collect();
        let b_hat_norm = vec_norm(&b_hat);

        let has_c = op.c_vals().is_some();
        let mut rom = Self {
            op,
            n,
            n_edges,
            interior_to_full,
            basis: Vec::new(),
            kv: Vec::new(),
            mv: Vec::new(),
            cv: has_c.then(Vec::new),
            spv: vec![Vec::new(); op.n_ports()],
            k_r: Vec::new(),
            m_r: Vec::new(),
            c_r: has_c.then(Vec::new),
            s_r: vec![Vec::new(); op.n_ports()],
            port_inv_z,
            b_hat,
            b_hat_r: Vec::new(),
            b_hat_norm,
            snapshot_omegas: Vec::new(),
            converged: false,
            worst_residual: f64::INFINITY,
        };

        // Zero drive: every solution is identically zero (matching the
        // dense sweep's zero-RHS semantics); nothing to sample.
        if rom.b_hat_norm == 0.0 {
            rom.converged = true;
            rom.worst_residual = 0.0;
            return Ok(rom);
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
                let eta = match rom.try_reduced_solve(omegas[i]) {
                    Some(x_r) => rom.residual_indicator(omegas[i], &x_r),
                    None => f64::INFINITY,
                };
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
            if !grew {
                // Snapshot linearly dependent on the current basis: the
                // candidate is consumed (never re-picked) but the ROM did
                // not change; keep going with the remaining candidates.
                continue;
            }
        }
        Ok(rom)
    }

    /// Snapshot frequencies in greedy selection order (one full-order
    /// solve each).
    pub fn snapshot_omegas(&self) -> &[f64] {
        &self.snapshot_omegas
    }

    /// Reduced dimension `k` (≤ number of snapshots; smaller when a
    /// snapshot was linearly dependent).
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
    /// # Errors
    ///
    /// [`RomError::ReducedSolveSingular`] if the reduced system has no
    /// solution at this frequency.
    pub fn evaluate(&self, omega: f64) -> Result<RomSweepPoint, RomError> {
        let k = self.basis.len();
        let x_r = if k == 0 {
            Vec::new()
        } else {
            self.try_reduced_solve(omega)
                .ok_or(RomError::ReducedSolveSingular { order: k, omega })?
        };
        let residual_indicator = self.residual_indicator(omega, &x_r);

        // Reconstruct x = V x_r and scatter to the full edge vector.
        let mut x_int = vec![c64::new(0.0, 0.0); self.n];
        for (j, v) in self.basis.iter().enumerate() {
            let xj = x_r[j];
            for (xi, &vi) in x_int.iter_mut().zip(v.iter()) {
                *xi += vi * xj;
            }
        }
        let mut e_edges = vec![c64::new(0.0, 0.0); self.n_edges];
        for (i, &full) in self.interior_to_full.iter().enumerate() {
            e_edges[full] = x_int[i];
        }

        let ports = (0..self.op.n_ports())
            .map(|p| {
                let v = self.op.port_voltage(p, &e_edges);
                let i = self.op.port_current(p, v);
                PortCircuit { v, i, z: v / i }
            })
            .collect();
        Ok(RomSweepPoint {
            omega,
            residual_indicator,
            ports,
        })
    }

    /// One full-order snapshot solve at `omega`, MGS-orthonormalized into
    /// the basis. Returns `Ok(false)` (without growing the ROM) when the
    /// snapshot is numerically dependent on the current basis.
    fn add_snapshot(&mut self, omega: f64) -> Result<bool, RomError> {
        self.snapshot_omegas.push(omega);
        let sol = self.op.factor_at(omega)?.solve()?;
        let mut w: Vec<c64> = self
            .interior_to_full
            .iter()
            .map(|&j| sol.e_edges[j])
            .collect();

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
            return Ok(false);
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
                let mut y = vec![c64::new(0.0, 0.0); self.n];
                for &(r, c, v) in data.mass_triplets {
                    y[r] += w[c] * v;
                }
                y
            })
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
        self.b_hat_r.push(dot_h(&w, &self.b_hat));

        // Commit the column.
        self.kv.push(kw);
        self.mv.push(mw);
        if let (Some(cv), Some(cw)) = (self.cv.as_mut(), cw) {
            cv.push(cw);
        }
        for (p, spw_p) in spw.into_iter().enumerate() {
            self.spv[p].push(spw_p);
        }
        self.basis.push(w);
        Ok(true)
    }

    /// Assemble and solve the reduced `A_r(ω) x_r = ω b̂_r`. `None` when
    /// the dense LU hits a zero/non-finite pivot.
    fn try_reduced_solve(&self, omega: f64) -> Option<Vec<c64>> {
        let k = self.basis.len();
        if k == 0 {
            return Some(Vec::new());
        }
        let omega2 = omega * omega;
        let i_omega = c64::new(0.0, omega);
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
                a[r * k + c] = v;
            }
        }
        let b: Vec<c64> = self.b_hat_r.iter().map(|&x| x * omega).collect();
        solve_dense_lu(a, b, k)
    }

    /// `η(ω) = ‖A(ω)Vx_r − ωb̂‖ / (ω‖b̂‖)` via the cached matrix–basis
    /// products — `O(nk)`, no sparse assembly.
    fn residual_indicator(&self, omega: f64, x_r: &[c64]) -> f64 {
        let omega2 = omega * omega;
        let mut r: Vec<c64> = self.b_hat.iter().map(|&b| b * (-omega)).collect();
        for (j, &xj) in x_r.iter().enumerate() {
            let sk = xj;
            let sm = xj * (-omega2);
            let sc = xj * c64::new(0.0, omega);
            for ((ri, &kvi), &mvi) in r.iter_mut().zip(self.kv[j].iter()).zip(self.mv[j].iter()) {
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
        }
        let num = vec_norm(&r);
        let den = omega.abs() * self.b_hat_norm;
        if den == 0.0 { num } else { num / den }
    }
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
/// `surfaces` must be empty in v1 (Leontovich walls carry a `√ω`
/// coefficient — see the module docs); pass `&[]`.
///
/// # Errors
///
/// [`RomError::UnsupportedOperator`] for a non-empty `surfaces`;
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
    if !surfaces.is_empty() {
        return Err(RomError::UnsupportedOperator {
            reason: format!(
                "{} Leontovich impedance surface(s) requested",
                surfaces.len()
            ),
        });
    }
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
