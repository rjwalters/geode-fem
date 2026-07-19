//! Impedance-extraction post-processing: `Z(ω) → L(ω), R(ω), Q(ω),
//! S-parameters` over port-driven solves (Epic #193, issue #203).
//!
//! Given a driven solve excited through a lumped port
//! ([`crate::driven::ports::LumpedPort`], issue #202), this module
//! reduces the field solution to the circuit quantities that are the
//! epic's actual deliverable:
//!
//! ```text
//! Z(ω)  = V / I                 (port input impedance; V from the
//!                                port-field projection, I from the
//!                                Thevenin admittance relation)
//! R(ω)  = Re Z(ω)               (series resistance)
//! L(ω)  = Im Z(ω) / ω           (series inductance)
//! Q(ω)  = Im Z(ω) / Re Z(ω)     (quality factor)
//! S₁₁(ω) = (Z − Z₀) / (Z + Z₀)  (reflection vs real reference Z₀)
//! ```
//!
//! plus self-resonance detection from the `Im Z(ω)` zero crossing when
//! a sweep brackets it.
//!
//! Everything here is **post-processing over [`crate::driven::solve::DrivenSolution`]** — no
//! new assembly physics. The field-to-circuit reduction reuses the
//! lumped-port flux functional `f_i = ∮ N_i · ê dS` (the same discrete
//! functional that drives the port, so the drive/measure pair is
//! adjoint-consistent; see `lumped_port.rs`).
//!
//! # Frequency sweeps
//!
//! `A(ω) = K + iωC − ω²M` re-forms per frequency by linear combination
//! of ω-independent matrices (the design rationale recorded in
//! PR #198), so the sweep driver [`driven_frequency_sweep`] assembles
//! once through [`DrivenOperator`] and then *re-factors per ω, never
//! re-assembles*. The ω-dependent complex coefficients of the port and
//! Leontovich surface terms (issue #204) are cheap host-side scalar
//! rescales applied inside [`DrivenOperator::solve_at`].
//!
//! # Multi-port S-parameters (issue #214)
//!
//! [`s_parameter_frequency_sweep`] extracts the full N-port S-matrix:
//! column `j` of `S(ω)` comes from driving port `j` (its `v_inc`) with
//! every other port passively terminated in its own reference `R`. At
//! a fixed ω all N excitations share one LU factorization
//! ([`DrivenOperator::factor_at`] +
//! [`crate::driven::solve::FactoredDrivenOperator::solve_excited`]), so an
//! N-port S-matrix costs one factorization + N back-substitutions per
//! frequency. The per-excitation port V/I readbacks assemble the
//! impedance matrix `Z = V·I⁻¹` and then
//!
//! ```text
//! S = F (Z − Z₀)(Z + Z₀)⁻¹ F⁻¹,    Z₀ = diag(R_k),  F = diag(1/√R_k),
//! ```
//!
//! the standard real-reference S-matrix with per-port reference
//! impedances (the √R normalization keeps `Sᵀ = S` for the reciprocal
//! systems this solver produces; it cancels when all references are
//! equal). The single-port path (`n = 1`) routes through the same
//! factorization machinery and the scalar [`s11`], reproducing the
//! [`driven_frequency_sweep`] S₁₁ bit-for-bit.

use burn::tensor::backend::Backend;
use faer::c64;

use crate::driven::ports::{LumpedPort, port_current, port_voltage};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
    SurfaceImpedanceBc,
};
use crate::mesh::TetMesh;

/// Circuit quantities of one port at one frequency, read off a driven
/// solution.
#[derive(Debug, Clone, Copy)]
pub struct PortCircuit {
    /// Port voltage `V = (1/w) ∮ E · ê dS`.
    pub v: c64,
    /// Port current `I = (2 V_inc − V) / R`.
    pub i: c64,
    /// Input impedance `Z = V / I`.
    pub z: c64,
}

impl PortCircuit {
    /// Series resistance `R(ω) = Re Z`.
    pub fn resistance(&self) -> f64 {
        self.z.re
    }

    /// Series inductance `L(ω) = Im Z / ω`.
    pub fn inductance(&self, omega: f64) -> f64 {
        inductance(self.z, omega)
    }

    /// Quality factor `Q(ω) = Im Z / Re Z`.
    pub fn quality_factor(&self) -> f64 {
        quality_factor(self.z)
    }

    /// Single-port reflection coefficient `S₁₁` vs the real reference
    /// impedance `z0`.
    pub fn s11(&self, z0: f64) -> c64 {
        s11(self.z, z0)
    }
}

/// Extract the port circuit quantities `V`, `I`, `Z` from a single
/// driven solution (`e_edges` in `mesh.edges()` order, e.g.
/// [`crate::driven::solve::DrivenSolution::e_edges`]).
///
/// Thin composition of [`crate::driven::ports::port_voltage`] and
/// [`crate::driven::ports::port_current`]; sweeps should prefer
/// [`driven_frequency_sweep`], which reuses the assembled operator and
/// the cached port flux across frequencies.
pub fn extract_port_circuit(
    mesh: &TetMesh,
    port: &LumpedPort<'_>,
    edges: &[[u32; 2]],
    e_edges: &[c64],
) -> PortCircuit {
    let v = port_voltage(mesh, port, edges, e_edges);
    let i = port_current(port, v);
    PortCircuit { v, i, z: v / i }
}

/// Series inductance `L(ω) = Im Z / ω`.
pub fn inductance(z: c64, omega: f64) -> f64 {
    z.im / omega
}

/// Quality factor `Q(ω) = Im Z / Re Z` (±∞ for a lossless reactance).
pub fn quality_factor(z: c64) -> f64 {
    z.im / z.re
}

/// Single-port reflection coefficient vs a **real** reference impedance:
///
/// ```text
/// S₁₁ = (Z − Z₀) / (Z + Z₀).
/// ```
///
/// Limits: `Z = Z₀` (matched) → 0; `Z → 0` (short) → −1;
/// `|Z| → ∞` (open) → +1.
pub fn s11(z: c64, z0: f64) -> c64 {
    (z - z0) / (z + z0)
}

/// Single-frequency N-port S-parameter matrix vs real per-port
/// reference impedances `Z₀ = diag(R_k)` (issue #214).
///
/// Construct from a port input impedance ([`SMatrix::from_single_port_z`],
/// single-port, exact) or from a full impedance matrix
/// ([`SMatrix::from_z_matrix`], N-port). The sweep driver
/// [`s_parameter_frequency_sweep`] produces one per frequency from
/// per-excitation port-driven solves.
#[derive(Debug, Clone)]
pub struct SMatrix {
    /// Real per-port reference impedances `Z₀ₖ` (length `n`).
    pub z0: Vec<f64>,
    /// Number of ports `n`.
    pub n_ports: usize,
    /// Row-major `n × n` entries.
    pub s: Vec<c64>,
}

impl SMatrix {
    /// Exact single-port S-matrix from the port input impedance:
    /// `S = [S₁₁]` with `S₁₁ = (Z − Z₀)/(Z + Z₀)`.
    pub fn from_single_port_z(z: c64, z0: f64) -> Self {
        Self {
            z0: vec![z0],
            n_ports: 1,
            s: vec![s11(z, z0)],
        }
    }

    /// N-port S-matrix from a row-major `n × n` impedance matrix `z`
    /// and real per-port reference impedances `z0` (`n = z0.len()`):
    ///
    /// ```text
    /// S = F (Z − Z₀)(Z + Z₀)⁻¹ F⁻¹,   Z₀ = diag(z0),  F = diag(1/√z0ₖ),
    /// ```
    ///
    /// the standard real-reference (power-wave) S-matrix. The `F`
    /// similarity is what keeps `Sᵀ = S` for a reciprocal (symmetric)
    /// `Z` with **unequal** references; with all `z0ₖ` equal it cancels
    /// and the formula reduces to the textbook
    /// `S = (Z − Z₀)(Z + Z₀)⁻¹`.
    ///
    /// The `n = 1` case delegates to [`SMatrix::from_single_port_z`]
    /// (same scalar arithmetic as [`s11`] — the bit-for-bit single-port
    /// guarantee of issue #214).
    ///
    /// # Panics
    ///
    /// Panics if `z.len() ≠ z0.len()²`, if `z0` is empty or contains a
    /// non-positive/non-finite reference, or if `Z + Z₀` is singular.
    pub fn from_z_matrix(z: &[c64], z0: &[f64]) -> Self {
        let n = z0.len();
        assert!(n > 0, "S-matrix needs at least one port");
        assert_eq!(
            z.len(),
            n * n,
            "Z must be a row-major n × n matrix with n = z0.len()"
        );
        assert!(
            z0.iter().all(|&r| r.is_finite() && r > 0.0),
            "reference impedances must be finite and positive"
        );
        if n == 1 {
            return Self::from_single_port_z(z[0], z0[0]);
        }

        // A = Z − Z₀, B = Z + Z₀ (Z₀ diagonal).
        let mut a = z.to_vec();
        let mut b = z.to_vec();
        for p in 0..n {
            a[p * n + p] -= z0[p];
            b[p * n + p] += z0[p];
        }
        let mut s = right_divide(&a, &b, n).expect("Z + Z0 must be non-singular");
        // Reference normalization: S ← F S F⁻¹, F = diag(1/√z0).
        for i in 0..n {
            for j in 0..n {
                s[i * n + j] *= (z0[j] / z0[i]).sqrt();
            }
        }
        Self {
            z0: z0.to_vec(),
            n_ports: n,
            s,
        }
    }

    /// Entry `S[i][j]` (0-based).
    ///
    /// # Panics
    ///
    /// Panics if `i` or `j` is out of range.
    pub fn entry(&self, i: usize, j: usize) -> c64 {
        assert!(i < self.n_ports && j < self.n_ports, "S-matrix index");
        self.s[i * self.n_ports + j]
    }
}

/// Gauss-Jordan inverse with partial pivoting of a row-major `n × n`
/// complex matrix. Returns `None` on an exactly singular pivot. The
/// port-count matrices this serves are tiny (n = number of ports), so
/// a dense elimination is the right tool.
fn invert_complex(m: &[c64], n: usize) -> Option<Vec<c64>> {
    debug_assert_eq!(m.len(), n * n);
    let mut a = m.to_vec();
    let mut inv: Vec<c64> = (0..n * n)
        .map(|idx| {
            if idx % (n + 1) == 0 {
                c64::new(1.0, 0.0)
            } else {
                c64::new(0.0, 0.0)
            }
        })
        .collect();
    for col in 0..n {
        // Partial pivot on the largest |a[r][col]|, r ≥ col.
        let mut piv = col;
        let mut piv_norm = a[col * n + col].norm();
        for r in (col + 1)..n {
            let v = a[r * n + col].norm();
            if v > piv_norm {
                piv = r;
                piv_norm = v;
            }
        }
        if piv_norm == 0.0 || piv_norm.is_nan() {
            return None;
        }
        if piv != col {
            for c in 0..n {
                a.swap(col * n + c, piv * n + c);
                inv.swap(col * n + c, piv * n + c);
            }
        }
        let d = a[col * n + col];
        for c in 0..n {
            a[col * n + c] /= d;
            inv[col * n + c] /= d;
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = a[r * n + col];
            if f.re == 0.0 && f.im == 0.0 {
                continue;
            }
            for c in 0..n {
                let av = a[col * n + c] * f;
                a[r * n + c] -= av;
                let iv = inv[col * n + c] * f;
                inv[r * n + c] -= iv;
            }
        }
    }
    Some(inv)
}

/// Right matrix division `A · B⁻¹` of row-major `n × n` complex
/// matrices. Returns `None` if `B` is singular.
fn right_divide(a: &[c64], b: &[c64], n: usize) -> Option<Vec<c64>> {
    debug_assert_eq!(a.len(), n * n);
    let b_inv = invert_complex(b, n)?;
    let mut out = vec![c64::new(0.0, 0.0); n * n];
    for r in 0..n {
        for c in 0..n {
            let mut acc = c64::new(0.0, 0.0);
            for k in 0..n {
                acc += a[r * n + k] * b_inv[k * n + c];
            }
            out[r * n + c] = acc;
        }
    }
    Some(out)
}

/// One frequency point of a port-driven sweep.
#[derive(Debug, Clone)]
pub struct SweepPoint {
    /// Frequency `ω ≡ k₀` (natural units, as in [`crate::driven`]).
    pub omega: f64,
    /// Post-solve relative residual at this frequency.
    pub residual_rel: f64,
    /// Per-port circuit quantities, in the order the ports were passed
    /// to the sweep.
    pub ports: Vec<PortCircuit>,
    /// Krylov iterations per RHS at this ω (issue #264). One entry per
    /// back-solve performed at this frequency. `0` on the direct path
    /// (no Krylov iteration); the per-RHS COCG count on the iterative
    /// path. For [`driven_frequency_sweep`] / [`driven_frequency_sweep_with_mode`]
    /// the sweep performs one back-solve per ω so this vector has length 1.
    pub iters_per_rhs: Vec<usize>,
}

/// Frequency-sweep driver over a port-driven structure: assemble the
/// ω-independent operator **once** ([`DrivenOperator::assemble`]), then
/// re-form + re-factor `A(ω)` and extract `V`, `I`, `Z` at every
/// requested frequency.
///
/// The expensive Burn volume assembly of `K`, `M(ε)`, `C(σ)` and the
/// source moments runs once for the whole sweep; per frequency only
/// scalar recombination, the sparse LU, and the port readouts remain.
/// One sweep point reproduces the corresponding single-ω
/// [`crate::driven::solve::driven_solve_with_ports`] /
/// [`crate::driven::solve::driven_solve_with_surface_impedance`] call exactly
/// (same arithmetic, same triplet stream).
///
/// `surfaces` composes Leontovich impedance walls (issue #204) into the
/// sweep; their ω-dependent scalar coefficient is re-evaluated at every
/// frequency, as that issue's sweep caveat requires. Pass `&[]` for
/// none.
///
/// # Errors
///
/// Any [`DrivenError`] from assembly or from the per-ω solves; the
/// sweep stops at the first failing frequency.
#[allow(clippy::too_many_arguments)]
pub fn driven_frequency_sweep<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    source: &CurrentSource,
    device: &B::Device,
) -> Result<Vec<SweepPoint>, DrivenError> {
    driven_frequency_sweep_with_mode::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        ports,
        surfaces,
        omegas,
        source,
        SolverMode::Direct,
        device,
    )
}

/// [`driven_frequency_sweep`] with an explicit [`SolverMode`] knob
/// (issue #264).
///
/// `SolverMode::Direct` is the historical path — factor `A(ω)` once per
/// ω with sparse LU and back-substitute the single port-driven RHS, so
/// `driven_frequency_sweep` is exactly this entry point with
/// `SolverMode::Direct`.
///
/// `SolverMode::Iterative(settings)` instead builds the Jacobi
/// preconditioner from `A(ω)` once per ω and runs a fresh
/// [`crate::solver::ksp::Cocg`] iteration for the single RHS — no
/// factorization, no fill-in. The per-RHS COCG iteration count is
/// surfaced in [`SweepPoint::iters_per_rhs`] so the regression test
/// (and downstream callers) can detect convergence degradation across
/// frequencies. See [`SolverMode`] for the documented trade-off.
///
/// The iterative path returns the same [`DrivenError`] variants as the
/// direct path, with [`DrivenError::Solve`] wrapping any
/// [`crate::solver::ksp::KspError`] (Krylov breakdown / non-convergence /
/// preconditioner setup failure).
#[allow(clippy::too_many_arguments)]
pub fn driven_frequency_sweep_with_mode<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    source: &CurrentSource,
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<SweepPoint>, DrivenError> {
    let op = DrivenOperator::assemble::<B>(
        mesh, materials, sigma_tet, bcs, ports, surfaces, source, device,
    )?;
    omegas
        .iter()
        .map(|&omega| {
            let solver = op.prepare_at::<B>(omega, solver_mode, device)?;
            let (sol, report) = solver.solve()?;
            let ports = (0..op.n_ports())
                .map(|p| {
                    let v = op.port_voltage(p, &sol.e_edges);
                    let i = op.port_current(p, v);
                    PortCircuit { v, i, z: v / i }
                })
                .collect();
            Ok(SweepPoint {
                omega,
                residual_rel: sol.residual_rel,
                ports,
                iters_per_rhs: vec![report.iters],
            })
        })
        .collect()
}

/// One frequency point of an N-port S-parameter sweep
/// ([`s_parameter_frequency_sweep`]).
#[derive(Debug, Clone)]
pub struct SParameterSweepPoint {
    /// Frequency `ω ≡ k₀` (natural units, as in [`crate::driven`]).
    pub omega: f64,
    /// Worst (largest) per-RHS relative residual over the N
    /// per-excitation solves at this frequency.
    pub residual_rel: f64,
    /// Row-major `n × n` impedance matrix `Z(ω) = V·I⁻¹` assembled from
    /// the per-excitation port V/I readbacks (`V[k][j]`, `I[k][j]` =
    /// voltage/current at port `k` when port `j` is excited).
    pub z: Vec<c64>,
    /// S-matrix `S = F(Z − Z₀)(Z + Z₀)⁻¹F⁻¹` vs the per-port reference
    /// impedances `Z₀ₖ = Rₖ` (each port's own termination resistance).
    pub s: SMatrix,
    /// Krylov iterations per RHS at this ω (issue #264). One entry per
    /// excited port, in excitation order. `0` on the direct path; the
    /// per-RHS COCG iteration count on the iterative path.
    pub iters_per_rhs: Vec<usize>,
}

/// N-port S-parameter sweep driver (issue #214): assemble the
/// ω-independent operator **once**, then per frequency factor `A(ω)`
/// **once** and back-substitute one RHS per excited port — port `j`
/// driven at its `v_inc` with every other port passively terminated in
/// its own `R` (the issue-#202 admittance term, already in `A(ω)`).
///
/// The per-excitation V/I readbacks assemble `Z(ω) = V·I⁻¹` and
/// `S(ω) = F(Z − Z₀)(Z + Z₀)⁻¹F⁻¹` with per-port references
/// `Z₀ₖ = Rₖ` ([`SMatrix::from_z_matrix`]). For a reciprocal structure
/// (every system this solver assembles is complex-symmetric) `S` is
/// symmetric to solver precision — `S₂₁ = S₁₂` is a free regression.
///
/// The structure must be **purely port-driven**: there is no volume
/// current source (an internal all-zero source is assembled), and every
/// port needs a non-zero `v_inc` to serve as an excitation. With a
/// single port this reproduces the [`driven_frequency_sweep`] →
/// [`s11`] reflection coefficient bit-for-bit (same factorization, same
/// RHS arithmetic, same scalar `S₁₁` formula).
///
/// # Errors
///
/// [`DrivenError::InvalidPort`] if `ports` is empty or any port has a
/// zero `v_inc`; any [`DrivenError`] from assembly or the per-ω
/// factorizations/solves (the sweep stops at the first failure);
/// [`DrivenError::Solve`] if the per-excitation current matrix `I` is
/// singular (no well-defined `Z`).
#[allow(clippy::too_many_arguments)]
pub fn s_parameter_frequency_sweep<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    device: &B::Device,
) -> Result<Vec<SParameterSweepPoint>, DrivenError> {
    s_parameter_frequency_sweep_with_mode::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        ports,
        surfaces,
        omegas,
        SolverMode::Direct,
        device,
    )
}

/// [`s_parameter_frequency_sweep`] with an explicit [`SolverMode`] knob
/// (issue #264).
///
/// At each ω the sweep runs `n_ports` back-solves through one
/// [`crate::driven::solve::DrivenLinearSolver`] handle (one LU factorization
/// on the direct path, one Jacobi preconditioner on the iterative path)
/// — see [`SolverMode`] for the trade-off. Per-RHS iteration counts
/// land in [`SParameterSweepPoint::iters_per_rhs`] for the regression
/// channel.
#[allow(clippy::too_many_arguments)]
pub fn s_parameter_frequency_sweep_with_mode<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<SParameterSweepPoint>, DrivenError> {
    if ports.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "S-parameter extraction needs at least one port".to_string(),
        });
    }
    for (index, port) in ports.iter().enumerate() {
        if port.v_inc == c64::new(0.0, 0.0) {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "every port needs a non-zero v_inc to serve as an S-parameter excitation"
                    .to_string(),
            });
        }
    }

    // Purely port-driven: zero volume current source.
    let zero_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        ports,
        surfaces,
        &zero_source,
        device,
    )?;
    let n = op.n_ports();
    let z0: Vec<f64> = (0..n).map(|p| op.port_resistance(p)).collect();

    omegas
        .iter()
        .map(|&omega| {
            // One solver-handle prep (LU factor on direct, Jacobi build
            // on iterative), N back-substitutions per excitation
            // (issue #214 multi-RHS pattern; issue #264 solver-mode knob).
            let solver = op.prepare_at::<B>(omega, solver_mode, device)?;
            let mut residual_rel = 0.0_f64;
            let mut iters_per_rhs: Vec<usize> = Vec::with_capacity(n);
            // v_mat[k][j] / i_mat[k][j]: port-k readback under excitation j.
            let mut v_mat = vec![c64::new(0.0, 0.0); n * n];
            let mut i_mat = vec![c64::new(0.0, 0.0); n * n];
            for j in 0..n {
                let (sol, report) = solver.solve_excited(j)?;
                residual_rel = residual_rel.max(sol.residual_rel);
                iters_per_rhs.push(report.iters);
                for k in 0..n {
                    let v = op.port_voltage(k, &sol.e_edges);
                    // Port k is driven only in its own excitation solve;
                    // elsewhere it is a passive termination (V_inc = 0).
                    let v_inc = if k == j {
                        op.port_v_inc(k)
                    } else {
                        c64::new(0.0, 0.0)
                    };
                    v_mat[k * n + j] = v;
                    i_mat[k * n + j] = op.port_current_with_v_inc(k, v_inc, v);
                }
            }
            // Z = V·I⁻¹. The n = 1 scalar V/I matches PortCircuit::z
            // bit-for-bit (issue-#214 single-port guarantee).
            let z = if n == 1 {
                vec![v_mat[0] / i_mat[0]]
            } else {
                right_divide(&v_mat, &i_mat, n).ok_or_else(|| {
                    DrivenError::Solve(format!(
                        "singular per-excitation port-current matrix at ω = {omega}: \
                         Z(ω) = V·I⁻¹ is not defined"
                    ))
                })?
            };
            let s = SMatrix::from_z_matrix(&z, &z0);
            Ok(SParameterSweepPoint {
                omega,
                residual_rel,
                z,
                s,
                iters_per_rhs,
            })
        })
        .collect()
}

/// All `Im Z(ω)` sign changes of a sampled impedance curve, located by
/// linear interpolation between consecutive samples (an exact-zero
/// sample reports its own ω).
///
/// A sign change marks either a **series-type resonance** (a true zero
/// of `Im Z` — for an inductor the inductive→capacitive `+ → −`
/// crossing is the self-resonant frequency) or a sign flip **through a
/// pole** (parallel anti-resonance, where `|Im Z|` blows up at the
/// bracketing samples instead of shrinking). Distinguishing the two
/// requires inspecting `|Im Z|` near the crossing or sweeping the
/// admittance instead; callers with lossy structures (finite `Re Z`)
/// see finite peaks in both cases and the interpolated ω remains a
/// useful bracket.
///
/// `omegas` and `zs` must have equal length and `omegas` must be
/// strictly increasing; non-finite samples are skipped.
pub fn im_z_zero_crossings(omegas: &[f64], zs: &[c64]) -> Vec<f64> {
    assert_eq!(omegas.len(), zs.len(), "omegas/zs length mismatch");
    let mut crossings = Vec::new();
    let mut prev: Option<(f64, f64)> = None; // (ω, Im Z)
    for (&omega, &z) in omegas.iter().zip(zs.iter()) {
        if !z.im.is_finite() {
            prev = None;
            continue;
        }
        if z.im == 0.0 {
            crossings.push(omega);
            prev = Some((omega, z.im));
            continue;
        }
        if let Some((w1, im1)) = prev
            && im1 != 0.0
            && im1.signum() != z.im.signum()
        {
            // Linear interpolation of the bracketed zero.
            crossings.push(w1 + (omega - w1) * im1 / (im1 - z.im));
        }
        prev = Some((omega, z.im));
    }
    crossings
}

/// Self-resonant frequency estimate: the first `Im Z(ω)` zero crossing
/// the sweep brackets ([`im_z_zero_crossings`]), or `None` if the sweep
/// does not bracket one.
pub fn detect_srf(omegas: &[f64], zs: &[c64]) -> Option<f64> {
    im_z_zero_crossings(omegas, zs).into_iter().next()
}

// ─────────────────────────────────────────────────────────────────────────
// |S11|² objective closure for the driven shape/material adjoint (issue #626).
// ─────────────────────────────────────────────────────────────────────────

/// The squared reflection magnitude `g = |S₁₁|²` **and its Wirtinger
/// derivative** `∂g/∂V` w.r.t. the (complex, holomorphic) port voltage `V`.
///
/// The scalar port chain is
///
/// ```text
/// I    = (2 V_inc − V) / R
/// Z    = V / I
/// S₁₁  = (Z − Z₀) / (Z + Z₀)
/// g    = |S₁₁|² = S₁₁ · conj(S₁₁)
/// ```
///
/// `S₁₁(V)` is a **holomorphic** (rational) function of `V` — the field
/// enters `g` only through this one holomorphic scalar. Treating `V` and
/// `V̄` as independent (Wirtinger calculus), `conj(S₁₁)` depends on `V̄`
/// alone, so `∂(conj S₁₁)/∂V = 0` and
///
/// ```text
/// ∂g/∂V = conj(S₁₁) · dS₁₁/dV,
/// dS₁₁/dV = (dS₁₁/dZ)(dZ/dV),
/// dS₁₁/dZ = 2 Z₀ / (Z + Z₀)²,
/// dZ/dV   = (I + V/R) / I².
/// ```
///
/// This is the single analytic scalar the [`s11_sq_objective`] closure
/// scatters onto the port-flux covector to form the full-length edge
/// cotangent. Returns `(g, ∂g/∂V)`.
///
/// # Panics / numerics
///
/// The chain is singular where `I = 0` (`V = 2 V_inc`, an open port) or
/// `Z = −Z₀`; callers keep the driven pencil away from these by evaluating
/// off the (lossless) cavity resonance. No panic is raised — the result is
/// simply `inf`/`nan` there, which the adjoint's residual check surfaces.
pub fn s11_sq_and_dg_dv(v: c64, v_inc: c64, r: f64, z0: f64) -> (f64, c64) {
    let inv_r = 1.0 / r;
    let i = (v_inc * 2.0 - v) * inv_r;
    let z = v / i;
    let zpz0 = z + z0;
    let s = (z - z0) / zpz0;
    let g = s.re * s.re + s.im * s.im;

    // dZ/dV = (I + V/R) / I²   and   dS₁₁/dZ = 2 Z₀ / (Z + Z₀)².
    let dz_dv = (i + v * inv_r) / (i * i);
    let ds_dz = c64::new(2.0 * z0, 0.0) / (zpz0 * zpz0);
    let ds_dv = ds_dz * dz_dv;
    let dg_dv = s.conj() * ds_dv;
    (g, dg_dv)
}

/// Build the **real** scalar objective closure `g(x) = |S₁₁(f₀)|²` (with
/// its Wirtinger cotangent) for the driven Nédélec shape/material adjoint
/// ([`crate::driven::shape::driven_shape_gradient`],
/// [`crate::driven::adjoint::driven_material_adjoint_gradient`]) — issue #626.
///
/// The field `x` (`[n_edges]` complex edge DOFs) enters the figure of merit
/// **only** through the single holomorphic linear functional
///
/// ```text
/// V = c · Σ_i f_i x_i,   c = 1/w  (`inv_width`),   f = `flux`
/// ```
///
/// where `f = `[`crate::driven::ports::assemble_port_flux`] is the real,
/// sparse port-flux covector (non-zero only on port-face edges) and `w` the
/// port width. Because `V` is holomorphic in `x` (`∂V/∂x_i = c f_i`,
/// `∂V̄/∂x_i = 0`), the full-length Wirtinger cotangent the adjoint API
/// wants collapses to
///
/// ```text
/// ∂g/∂x_i = (∂g/∂V) · c · f_i          (nonzero only on port-face edges)
/// ```
///
/// with `∂g/∂V` the single analytic scalar from [`s11_sq_and_dg_dv`]. No new
/// adjoint and no new assembly: the closure is a thin wrapper over the
/// existing port-flux primitive and the scalar S-parameter chain, matching
/// the `Fn(&[c64]) -> (f64, Vec<c64>)` contract (`∂g/∂x_i` un-conjugated, PEC
/// cotangent entries ignored by the adjoint).
///
/// `flux` must be full length (`[n_edges]`); the returned cotangent has the
/// same length.
///
/// * `flux` — the port-flux covector `f_i = ∮_{Γp} N_i·ê dS`.
/// * `inv_width` — `c = 1/w`.
/// * `v_inc` — incident (drive) voltage `V_inc`.
/// * `resistance` — lumped port resistance `R` (natural units).
/// * `z0` — reference impedance `Z₀` for `S₁₁` (natural units; for the
///   Palace-style uniform port `Z₀ = R`).
pub fn s11_sq_objective(
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    resistance: f64,
    z0: f64,
) -> impl Fn(&[c64]) -> (f64, Vec<c64>) {
    move |x: &[c64]| {
        // V = c · Σ f_i x_i (the single holomorphic port functional).
        let mut v = c64::new(0.0, 0.0);
        for (f, xi) in flux.iter().zip(x.iter()) {
            v += *xi * *f;
        }
        v *= inv_width;

        let (g, dg_dv) = s11_sq_and_dg_dv(v, v_inc, resistance, z0);

        // Scatter: ∂g/∂x_i = (∂g/∂V) · c · f_i (sparse on port-face edges).
        let scale = dg_dv * inv_width;
        let cot: Vec<c64> = flux.iter().map(|&f| scale * f).collect();
        (g, cot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(re: f64, im: f64) -> c64 {
        c64::new(re, im)
    }

    /// R/L/Q definitions on a synthetic impedance.
    #[test]
    fn circuit_quantities_match_definitions() {
        let omega = 2.0;
        let z = c(0.5, 3.0);
        let pc = PortCircuit {
            v: c(1.0, 0.0),
            i: c(1.0, 0.0),
            z,
        };
        assert_eq!(pc.resistance(), 0.5);
        assert_eq!(pc.inductance(omega), 1.5);
        assert_eq!(pc.quality_factor(), 6.0);
        assert_eq!(inductance(z, omega), 1.5);
        assert_eq!(quality_factor(z), 6.0);
    }

    /// S₁₁ limits: matched → 0, short → −1, open → +1.
    #[test]
    fn s11_limits() {
        let z0 = 50.0;
        assert!(s11(c(50.0, 0.0), z0).norm() < 1e-15);
        assert!((s11(c(0.0, 0.0), z0) - c(-1.0, 0.0)).norm() < 1e-15);
        let open = s11(c(1e12, 0.0), z0);
        assert!((open - c(1.0, 0.0)).norm() < 1e-9);
        // Lossless reactance reflects with unit magnitude.
        let reactive = s11(c(0.0, 17.0), z0);
        assert!((reactive.norm() - 1.0).abs() < 1e-15);
    }

    /// Single-port S-matrix is the scalar S₁₁.
    #[test]
    fn single_port_s_matrix() {
        let z = c(25.0, 10.0);
        let m = SMatrix::from_single_port_z(z, 50.0);
        assert_eq!(m.n_ports, 1);
        assert_eq!(m.entry(0, 0), s11(z, 50.0));
    }

    /// `from_z_matrix` with n = 1 delegates to the scalar path —
    /// bit-for-bit identical to `from_single_port_z` (issue #214).
    #[test]
    fn one_port_z_matrix_is_bitwise_single_port() {
        let z = c(25.0, 10.0);
        let m = SMatrix::from_z_matrix(&[z], &[50.0]);
        let m1 = SMatrix::from_single_port_z(z, 50.0);
        assert_eq!(m.n_ports, 1);
        assert_eq!(m.z0, m1.z0);
        assert_eq!(m.entry(0, 0), m1.entry(0, 0));
    }

    /// A matched impedance matrix `Z = diag(Z₀)` is reflectionless and
    /// isolation-free: `S = 0`.
    #[test]
    fn matched_z_matrix_gives_zero_s() {
        let z0 = [50.0, 75.0];
        let z = [c(50.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(75.0, 0.0)];
        let m = SMatrix::from_z_matrix(&z, &z0);
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    m.entry(i, j).norm() < 1e-15,
                    "matched S[{i}][{j}] = {} must vanish",
                    m.entry(i, j)
                );
            }
        }
    }

    /// A diagonal (uncoupled) Z-matrix with distinct per-port
    /// references reduces to independent scalar reflections.
    #[test]
    fn diagonal_z_matrix_reduces_to_per_port_s11() {
        let z0 = [50.0, 75.0];
        let (z1, z2) = (c(30.0, 12.0), c(100.0, -40.0));
        let z = [z1, c(0.0, 0.0), c(0.0, 0.0), z2];
        let m = SMatrix::from_z_matrix(&z, &z0);
        assert!((m.entry(0, 0) - s11(z1, z0[0])).norm() < 1e-15);
        assert!((m.entry(1, 1) - s11(z2, z0[1])).norm() < 1e-15);
        assert!(m.entry(0, 1).norm() < 1e-15);
        assert!(m.entry(1, 0).norm() < 1e-15);
    }

    /// Shunt-impedance two-port (`Z` all-ones × z_p) vs the textbook
    /// result `S₁₁ = −Z₀/(Z₀ + 2 z_p)`, `S₂₁ = 2 z_p/(Z₀ + 2 z_p)`
    /// (equal references).
    #[test]
    fn shunt_impedance_two_port_matches_textbook() {
        let z0 = 50.0;
        let z_p = c(20.0, 35.0);
        let z = [z_p, z_p, z_p, z_p];
        let m = SMatrix::from_z_matrix(&z, &[z0, z0]);
        let denom = z_p * 2.0 + z0;
        let s11_ref = c(-z0, 0.0) / denom;
        let s21_ref = z_p * 2.0 / denom;
        assert!((m.entry(0, 0) - s11_ref).norm() < 1e-14);
        assert!((m.entry(1, 1) - s11_ref).norm() < 1e-14);
        assert!((m.entry(0, 1) - s21_ref).norm() < 1e-14);
        assert!((m.entry(1, 0) - s21_ref).norm() < 1e-14);
    }

    /// Reciprocity is preserved under **unequal** per-port references:
    /// a symmetric Z must produce a symmetric S (this is exactly what
    /// the `F = diag(1/√Z₀)` normalization buys; the unnormalized
    /// `(Z − Z₀)(Z + Z₀)⁻¹` is not symmetric here).
    #[test]
    fn symmetric_z_gives_symmetric_s_with_unequal_references() {
        let z0 = [1.0, 2.0];
        let zm = c(0.5, 0.1); // mutual
        let z = [c(1.0, 2.0), zm, zm, c(3.0, -1.0)];
        let m = SMatrix::from_z_matrix(&z, &z0);
        let asym = (m.entry(0, 1) - m.entry(1, 0)).norm();
        assert!(
            asym < 1e-15 * m.entry(0, 1).norm().max(1.0),
            "Sᵀ ≠ S for symmetric Z: |S12 − S21| = {asym}"
        );
    }

    /// The dense helpers: `A·A⁻¹ = I` for a well-conditioned complex
    /// matrix, and an exactly singular matrix is reported as `None`.
    #[test]
    fn invert_and_right_divide_helpers() {
        let a = [c(2.0, 1.0), c(0.5, -0.3), c(-1.0, 0.2), c(1.5, 2.5)];
        let ident = right_divide(&a, &a, 2).expect("non-singular");
        for i in 0..2 {
            for j in 0..2 {
                let want = if i == j { c(1.0, 0.0) } else { c(0.0, 0.0) };
                assert!((ident[i * 2 + j] - want).norm() < 1e-15);
            }
        }
        // Rank-1 (singular) matrix.
        let s = [c(1.0, 0.0), c(2.0, 0.0), c(2.0, 0.0), c(4.0, 0.0)];
        assert!(invert_complex(&s, 2).is_none());
    }

    /// SRF detection on a series-LC impedance `Im Z = ωL − 1/(ωC)`:
    /// the analytic resonance `ω₀ = 1/√(LC)` is bracketed and located
    /// to interpolation accuracy.
    #[test]
    fn detects_series_resonance_zero_crossing() {
        let (l, cap) = (2.0_f64, 0.125_f64);
        let omega0 = 1.0 / (l * cap).sqrt(); // = 2.0
        let omegas: Vec<f64> = (1..=12).map(|k| 0.3 * k as f64).collect();
        let zs: Vec<c64> = omegas
            .iter()
            .map(|&w| c(0.01, l * w - 1.0 / (cap * w)))
            .collect();
        let srf = detect_srf(&omegas, &zs).expect("sweep brackets the resonance");
        assert!(
            (srf - omega0).abs() < 0.05,
            "series-LC SRF: got {srf}, want {omega0}"
        );
    }

    /// A monotone inductive curve has no crossing; a sweep that does
    /// not bracket the resonance returns `None`.
    #[test]
    fn no_crossing_returns_none() {
        let omegas = [1.0, 2.0, 3.0];
        let zs = [c(0.1, 1.0), c(0.1, 2.0), c(0.1, 3.0)];
        assert!(detect_srf(&omegas, &zs).is_none());
        assert!(im_z_zero_crossings(&omegas, &zs).is_empty());
    }

    /// An exact-zero sample reports its own ω; multiple crossings are
    /// all reported in order.
    #[test]
    fn exact_zero_and_multiple_crossings() {
        let omegas = [1.0, 2.0, 3.0, 4.0];
        let zs = [c(0.1, -1.0), c(0.1, 0.0), c(0.1, 1.0), c(0.1, -1.0)];
        let crossings = im_z_zero_crossings(&omegas, &zs);
        assert_eq!(crossings.len(), 2);
        assert_eq!(crossings[0], 2.0);
        assert!((crossings[1] - 3.5).abs() < 1e-15);
    }

    // ── |S11|² objective closure (issue #626) ────────────────────────────

    /// `g = |S11|²` must equal `|s11(Z, Z₀)|²` with `Z` from the port
    /// circuit chain, and vanish at the matched voltage `V = V_inc`.
    #[test]
    fn s11_sq_value_matches_port_chain() {
        let v_inc = c(1.0, 0.0);
        let r = 1.3;
        let z0 = 1.3;
        // Matched: V = V_inc ⇒ Z = R = Z₀ ⇒ g = 0.
        let (g0, _) = s11_sq_and_dg_dv(v_inc, v_inc, r, z0);
        assert!(g0 < 1e-28, "matched |S11|² should be ~0, got {g0}");
        // Arbitrary V: cross-check against the standalone s11() helper.
        let v = c(0.7, -0.4);
        let i = (v_inc * 2.0 - v) * (1.0 / r);
        let z = v / i;
        let (g, _) = s11_sq_and_dg_dv(v, v_inc, r, z0);
        let g_ref = s11(z, z0).norm().powi(2);
        assert!((g - g_ref).abs() < 1e-14, "|S11|² {g} vs {g_ref}");
    }

    /// **Isolated `∂g/∂V` gate.** The analytic Wirtinger derivative
    /// `∂g/∂V = conj(S11)·dS11/dV` must match a central finite-difference
    /// Wirtinger estimate `½(∂g/∂V_re − i ∂g/∂V_im)` to ≤ 1e-9 — the issue
    /// #626 acceptance criterion. (A central FD on the analytic scalar chain
    /// is equivalent in accuracy to a complex-step here and needs no
    /// holomorphic real-axis restriction.)
    #[test]
    fn dg_dv_matches_finite_difference() {
        let v_inc = c(1.0, 0.0);
        let r = 1.3;
        let z0 = 1.3;
        let h = 1e-6;
        // Several off-matched probe voltages (non-singular I, Z ≠ −Z₀).
        for &v in &[c(0.7, -0.4), c(1.4, 0.9), c(0.2, 1.1), c(-0.5, 0.3)] {
            let (_, dg_dv) = s11_sq_and_dg_dv(v, v_inc, r, z0);
            let g = |vv: c64| s11_sq_and_dg_dv(vv, v_inc, r, z0).0;
            let dgd_re = (g(v + c(h, 0.0)) - g(v - c(h, 0.0))) / (2.0 * h);
            let dgd_im = (g(v + c(0.0, h)) - g(v - c(0.0, h))) / (2.0 * h);
            let fd = c(0.5 * dgd_re, -0.5 * dgd_im); // ½(∂/∂a − i ∂/∂b)
            let rel = (dg_dv - fd).norm() / fd.norm().max(1e-30);
            assert!(
                rel < 1e-9,
                "∂g/∂V at V={v}: analytic {dg_dv} vs FD {fd}, rel {rel:.3e}"
            );
        }
    }

    /// **Closure cotangent gate.** The full-length cotangent
    /// `∂g/∂x_i = (∂g/∂V)·c·f_i` the [`s11_sq_objective`] closure returns
    /// must match a central-FD Wirtinger derivative of `g(x)` w.r.t. each
    /// edge DOF `x_i` on a fixed field, to ≤ 1e-7 — proving the sparse
    /// scatter onto the port-flux covector is correct.
    #[test]
    fn objective_cotangent_matches_forward_fd() {
        // A short, dense synthetic flux + field (a stand-in for the port
        // edges of a real mesh) — the closure is mesh-agnostic.
        let flux = vec![0.3_f64, -0.15, 0.0, 0.22, -0.4, 0.0, 0.11];
        let inv_width = 1.0 / 1.7;
        let v_inc = c(1.0, 0.0);
        let r = 1.3;
        let z0 = 1.3;
        let obj = s11_sq_objective(flux.clone(), inv_width, v_inc, r, z0);

        let x = vec![
            c(0.10, 0.20),
            c(-0.30, 0.05),
            c(0.44, -0.12),
            c(0.02, 0.31),
            c(-0.18, -0.09),
            c(0.27, 0.14),
            c(0.06, -0.22),
        ];
        let (_, cot) = obj(&x);

        let h = 1e-6;
        let g_at = |xx: &[c64]| obj(xx).0;
        for i in 0..x.len() {
            let mut xp = x.clone();
            let mut xm = x.clone();
            xp[i] += c(h, 0.0);
            xm[i] -= c(h, 0.0);
            let dgd_re = (g_at(&xp) - g_at(&xm)) / (2.0 * h);
            let mut xp = x.clone();
            let mut xm = x.clone();
            xp[i] += c(0.0, h);
            xm[i] -= c(0.0, h);
            let dgd_im = (g_at(&xp) - g_at(&xm)) / (2.0 * h);
            let fd = c(0.5 * dgd_re, -0.5 * dgd_im);
            let rel = (cot[i] - fd).norm() / fd.norm().max(1e-12);
            // Off-flux edges (f_i = 0) must carry an exact-zero cotangent.
            if flux[i] == 0.0 {
                assert_eq!(cot[i], c(0.0, 0.0));
            } else {
                assert!(
                    rel < 1e-7,
                    "cotangent[{i}]: closure {} vs FD {fd}, rel {rel:.3e}",
                    cot[i]
                );
            }
        }
    }
}
