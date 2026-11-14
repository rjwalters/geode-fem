//! Electro-quasi-static (EQS) transient and small-signal solves on a
//! tetrahedral mesh (issue #1478).
//!
//! Solves `∇·(σ∇φ + ∂ₜ(ε∇φ)) = 0` for structures that carry both a
//! permittivity `ε` (F/m) and a small conductivity `σ` (S/m), on the same
//! scalar P1 kernel as [`crate::assembly::electrostatic`] and
//! [`crate::assembly::conduction`] ([`tet_p1_local`]): the semi-discrete
//! system is `C φ̇ + K φ = 0`, with `C` the permittivity stiffness and `K`
//! the conductance stiffness.
//!
//! # Terminals and floating conductors
//!
//! * A **driven terminal** is a named node set pinned to a prescribed
//!   waveform `V_i(t)`.
//! * A **floating conductor** is a named node set merged into one equipotential
//!   unknown whose *total* current (conduction + displacement) is zero. Its
//!   initial charge is zero.
//!
//! # Time stepping
//!
//! The θ-method, `(C/Δt + θK) φⁿ⁺¹ = (C/Δt − (1−θ)K) φⁿ`, with
//! `θ = 1` (backward Euler, first order, L-stable) the default and `θ = ½`
//! (Crank–Nicolson, second order) available. Any `θ ∈ [½, 1]` is
//! unconditionally stable. The matrix is constant for fixed `Δt`, so it is
//! factored once. Explicit stepping is not offered: the system is stiff.
//!
//! The initial state is the **electrostatic** solution for the waveform at
//! `t = 0` (`C φ = 0` with the floating conductors uncharged). Applying a
//! step at `t = 0` therefore starts from the capacitive divider; as
//! `t → ∞` with constant drive the solution tends to the DC conduction
//! solution of [`crate::assembly::conduction`].
//!
//! # Outputs
//!
//! [`EqsHistory`] holds, per step: the terminal voltages, floating-conductor
//! potentials, terminal currents split into conduction (`Σ (Kφ)`) and
//! displacement (`Σ C Δφ/Δt`) parts (A, positive into the network), the
//! volume-averaged `E` in each named region, and a charge-conservation
//! residual. At `t = 0` the displacement current is impulsive and is
//! reported as zero.
//!
//! # Frequency domain
//!
//! [`eqs_admittance`] solves `(K + jωC) φ = 0` per frequency (as a real
//! `2n × 2n` block system) and returns the terminal admittance
//! `Y = G + jωC`, the complex-permittivity form `ε − jσ/ω`.

use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::electrostatic::{ElectrostaticError, coords_of, tet_bary_grads, tet_p1_local};
use crate::mesh::TetMesh;
use crate::sparse_ops::{lu_solve as sparse_lu_solve, spmv};

type Csc = SparseColMat<usize, f64>;
type Lu = faer::sparse::linalg::solvers::Lu<usize, f64>;

/// A driven terminal: a named node set pinned to a prescribed waveform.
#[derive(Debug, Clone, PartialEq)]
pub struct EqsTerminal {
    /// Terminal name.
    pub name: String,
    /// 0-based node indices; non-empty and disjoint from every other
    /// terminal and floating conductor.
    pub nodes: Vec<u32>,
}

/// A floating (isolated, initially uncharged) conductor.
#[derive(Debug, Clone, PartialEq)]
pub struct EqsFloating {
    /// Conductor name.
    pub name: String,
    /// 0-based node indices; same constraints as [`EqsTerminal::nodes`].
    pub nodes: Vec<u32>,
}

/// A named set of tets over which the mean field is reported.
#[derive(Debug, Clone, PartialEq)]
pub struct EqsRegion {
    /// Region name.
    pub name: String,
    /// Tet indices; non-empty.
    pub tets: Vec<usize>,
}

/// Time-integration parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqsOptions {
    /// Time step (s), finite and `> 0`.
    pub dt: f64,
    /// θ-method weight in `[0.5, 1]`; `1` is backward Euler.
    pub theta: f64,
}

impl Default for EqsOptions {
    fn default() -> Self {
        Self {
            dt: 1.0,
            theta: 1.0,
        }
    }
}

/// One time level of an [`EqsHistory`].
#[derive(Debug, Clone)]
pub struct EqsStep {
    /// Time (s).
    pub t: f64,
    /// Applied terminal voltages (V).
    pub v: Vec<f64>,
    /// Potential (V) of each floating conductor.
    pub floating_phi: Vec<f64>,
    /// Conduction current (A) into the network at each driven terminal.
    pub i_cond: Vec<f64>,
    /// Displacement current (A) into the network at each driven terminal
    /// (zero at `t = 0`, where it is impulsive).
    pub i_disp: Vec<f64>,
    /// Total (conduction + displacement) current into each floating
    /// conductor (A); zero up to solver round-off for `t > 0`.
    pub floating_current: Vec<f64>,
    /// Volume-averaged `E = −∇φ` (V/m) per requested region.
    pub region_e: Vec<[f64; 3]>,
    /// `|Σ_terminals I + Σ_floating I| / max|I|` for `t > 0`; `None` at
    /// `t = 0`.
    pub charge_residual: Option<f64>,
}

/// Result of [`EqsSystem::run`].
#[derive(Debug, Clone)]
pub struct EqsHistory {
    /// Terminal names in column order.
    pub terminal_names: Vec<String>,
    /// Floating-conductor names.
    pub floating_names: Vec<String>,
    /// Region names.
    pub region_names: Vec<String>,
    /// `steps[0]` is `t = 0`; `steps[n]` is `t = n·Δt`.
    pub steps: Vec<EqsStep>,
    /// Final nodal potential (V).
    pub phi_final: Vec<f64>,
}

/// Shared assembled operators.
#[derive(Debug)]
struct Ops {
    n_nodes: usize,
    n_tets: usize,
    term_names: Vec<String>,
    term_nodes: Vec<Vec<u32>>,
    float_names: Vec<String>,
    float_nodes: Vec<Vec<u32>>,
    dof_of_node: Vec<Option<usize>>,
    n_dof: usize,
    c: Csc,
    k: Csc,
}

fn shape(msg: String) -> ElectrostaticError {
    ElectrostaticError::ShapeMismatch(msg)
}

fn combine(a: &Csc, ca: f64, b: &Csc, cb: f64) -> Result<Csc, ElectrostaticError> {
    let n = a.nrows();
    let mut t: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for (m, c) in [(a, ca), (b, cb)] {
        let r = m.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        for j in 0..n {
            for k in cp[j]..cp[j + 1] {
                t.push(Triplet::new(ri[k], j, c * v[k]));
            }
        }
    }
    Csc::try_new_from_triplets(n, n, &t).map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))
}

/// Allocating form of the shared sparse LU solve.
fn lu_solve(lu: &Lu, b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; b.len()];
    sparse_lu_solve(lu, b, &mut out);
    out
}

fn factor(a: &Csc) -> Result<Lu, ElectrostaticError> {
    a.as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))
}

fn check_finite(x: &[f64], what: &str) -> Result<(), ElectrostaticError> {
    if x.iter().any(|v| !v.is_finite()) {
        return Err(ElectrostaticError::Factorization(format!(
            "{what}: non-finite solution (singular system, e.g. an isolated \
             conductor with neither ε nor σ)"
        )));
    }
    Ok(())
}

impl Ops {
    fn build(
        mesh: &TetMesh,
        eps: &[f64],
        sigma: &[f64],
        terminals: &[EqsTerminal],
        floating: &[EqsFloating],
    ) -> Result<Self, ElectrostaticError> {
        let n_nodes = mesh.n_nodes();
        let n_tets = mesh.n_tets();
        for (name, v) in [("eps", eps), ("sigma", sigma)] {
            if v.len() != n_tets {
                return Err(shape(format!(
                    "{name} length {} != tet count {n_tets}",
                    v.len()
                )));
            }
            if let Some(x) = v.iter().find(|x| !x.is_finite() || **x < 0.0) {
                return Err(shape(format!("{name} must be finite and >= 0, got {x}")));
            }
        }
        if terminals.is_empty() {
            return Err(shape("no driven terminals".into()));
        }
        // 0..terminals = driven, then floating.
        let mut owner: Vec<Option<usize>> = vec![None; n_nodes];
        let sets = terminals
            .iter()
            .map(|t| (&t.name, &t.nodes))
            .chain(floating.iter().map(|f| (&f.name, &f.nodes)));
        for (gi, (name, nodes)) in sets.enumerate() {
            if nodes.is_empty() {
                return Err(ElectrostaticError::EmptyElectrode(name.clone()));
            }
            for &n in nodes {
                let n = n as usize;
                if n >= n_nodes {
                    return Err(shape(format!("{name}: node {n} >= node count {n_nodes}")));
                }
                if owner[n].is_some_and(|o| o != gi) {
                    return Err(shape(format!(
                        "node {n} belongs to more than one conductor ({name})"
                    )));
                }
                owner[n] = Some(gi);
            }
        }

        let mut tc: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
        let mut tk: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
        for (t, tet) in mesh.tets.iter().enumerate() {
            let (kl, _m, _v) = tet_p1_local(&coords_of(mesh, tet));
            for p in 0..4 {
                for q in 0..4 {
                    let (i, j) = (tet[p] as usize, tet[q] as usize);
                    if eps[t] != 0.0 {
                        tc.push(Triplet::new(i, j, eps[t] * kl[p][q]));
                    }
                    if sigma[t] != 0.0 {
                        tk.push(Triplet::new(i, j, sigma[t] * kl[p][q]));
                    }
                }
            }
        }
        let c = Csc::try_new_from_triplets(n_nodes, n_nodes, &tc)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
        let k = Csc::try_new_from_triplets(n_nodes, n_nodes, &tk)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

        let mut dof_of_node: Vec<Option<usize>> = vec![None; n_nodes];
        let mut n_dof = 0usize;
        for f in floating {
            for &n in &f.nodes {
                dof_of_node[n as usize] = Some(n_dof);
            }
            n_dof += 1;
        }
        for n in 0..n_nodes {
            if owner[n].is_none() {
                dof_of_node[n] = Some(n_dof);
                n_dof += 1;
            }
        }
        Ok(Self {
            n_nodes,
            n_tets,
            term_names: terminals.iter().map(|t| t.name.clone()).collect(),
            term_nodes: terminals.iter().map(|t| t.nodes.clone()).collect(),
            float_names: floating.iter().map(|f| f.name.clone()).collect(),
            float_nodes: floating.iter().map(|f| f.nodes.clone()).collect(),
            dof_of_node,
            n_dof,
            c,
            k,
        })
    }

    fn check_mesh(&self, mesh: &TetMesh) -> Result<(), ElectrostaticError> {
        if mesh.n_nodes() != self.n_nodes || mesh.n_tets() != self.n_tets {
            return Err(shape("mesh differs from the assembled system".into()));
        }
        Ok(())
    }

    /// Drop pinned rows/cols and merge floating nodes (sums duplicates).
    fn reduce_matrix(&self, a: &Csc) -> Result<Csc, ElectrostaticError> {
        let r = a.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        let mut t = Vec::with_capacity(v.len());
        for j in 0..self.n_nodes {
            let Some(dj) = self.dof_of_node[j] else {
                continue;
            };
            for k in cp[j]..cp[j + 1] {
                if let Some(di) = self.dof_of_node[ri[k]] {
                    t.push(Triplet::new(di, dj, v[k]));
                }
            }
        }
        Csc::try_new_from_triplets(self.n_dof, self.n_dof, &t)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))
    }

    fn reduce_vec(&self, full: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; self.n_dof];
        for (n, d) in self.dof_of_node.iter().enumerate() {
            if let Some(d) = d {
                out[*d] += full[n];
            }
        }
        out
    }

    /// Pinned-node vector: terminal voltages on terminal nodes, 0 elsewhere.
    fn pinned(&self, v: &[f64]) -> Vec<f64> {
        let mut w = vec![0.0; self.n_nodes];
        for (ti, nodes) in self.term_nodes.iter().enumerate() {
            for &n in nodes {
                w[n as usize] = v[ti];
            }
        }
        w
    }

    /// Pinned values plus the reduced unknowns spread back to all nodes.
    fn expand(&self, u: &[f64], w: &[f64]) -> Vec<f64> {
        let mut phi = w.to_vec();
        for (n, d) in self.dof_of_node.iter().enumerate() {
            if let Some(d) = d {
                phi[n] = u[*d];
            }
        }
        phi
    }

    fn sum_over(nodes: &[u32], x: &[f64]) -> f64 {
        nodes.iter().map(|&n| x[n as usize]).sum()
    }
}

/// An assembled and factored EQS transient problem.
#[derive(Debug)]
pub struct EqsSystem {
    ops: Ops,
    opts: EqsOptions,
    a_full: Csc,
    b_full: Csc,
    lu_a: Lu,
    lu_c: Lu,
}

/// Assemble and factor the EQS transient problem.
///
/// `eps` is the per-tet absolute permittivity (F/m) and `sigma` the per-tet
/// conductivity (S/m); both finite and `≥ 0`. Material `ε` should be `> 0`
/// wherever the initial electrostatic state must be defined.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] for bad lengths, values, indices,
/// overlapping conductors, `dt <= 0` or `θ ∉ [0.5, 1]`;
/// [`ElectrostaticError::EmptyElectrode`]; [`ElectrostaticError::Assembly`] /
/// [`ElectrostaticError::Factorization`] from faer.
pub fn assemble_eqs(
    mesh: &TetMesh,
    eps: &[f64],
    sigma: &[f64],
    terminals: &[EqsTerminal],
    floating: &[EqsFloating],
    opts: EqsOptions,
) -> Result<EqsSystem, ElectrostaticError> {
    if !(opts.dt.is_finite() && opts.dt > 0.0) {
        return Err(shape(format!("dt must be finite and > 0, got {}", opts.dt)));
    }
    if !(0.5..=1.0).contains(&opts.theta) {
        return Err(shape(format!(
            "theta must be in [0.5, 1], got {}",
            opts.theta
        )));
    }
    let ops = Ops::build(mesh, eps, sigma, terminals, floating)?;
    let inv = 1.0 / opts.dt;
    let a_full = combine(&ops.c, inv, &ops.k, opts.theta)?;
    let b_full = combine(&ops.c, inv, &ops.k, -(1.0 - opts.theta))?;
    let lu_a = factor(&ops.reduce_matrix(&a_full)?)?;
    let lu_c = factor(&ops.reduce_matrix(&ops.c)?)?;
    Ok(EqsSystem {
        ops,
        opts,
        a_full,
        b_full,
        lu_a,
        lu_c,
    })
}

impl EqsSystem {
    /// Number of driven terminals.
    pub fn n_terminals(&self) -> usize {
        self.ops.term_names.len()
    }

    /// Electrostatic state (`C φ = 0`, uncharged floating conductors) for
    /// terminal voltages `v`.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] on a length mismatch;
    /// [`ElectrostaticError::Factorization`] if the capacitive system is
    /// singular.
    pub fn electrostatic_state(&self, v: &[f64]) -> Result<Vec<f64>, ElectrostaticError> {
        if v.len() != self.n_terminals() {
            return Err(shape(format!(
                "voltages length {} != terminal count {}",
                v.len(),
                self.n_terminals()
            )));
        }
        let w = self.ops.pinned(v);
        let rhs: Vec<f64> = self
            .ops
            .reduce_vec(&spmv(self.ops.c.as_ref(), &w))
            .iter()
            .map(|x| -x)
            .collect();
        let u = lu_solve(&self.lu_c, &rhs);
        check_finite(&u, "electrostatic initial state")?;
        Ok(self.ops.expand(&u, &w))
    }

    /// Advance `n_steps` steps of `Δt` under `waveform(t) -> V_i`.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] for a mesh mismatch, a waveform
    /// of the wrong length or non-finite value, or a bad region;
    /// [`ElectrostaticError::Factorization`] for a singular initial state.
    pub fn run(
        &self,
        mesh: &TetMesh,
        waveform: &dyn Fn(f64) -> Vec<f64>,
        n_steps: usize,
        regions: &[EqsRegion],
    ) -> Result<EqsHistory, ElectrostaticError> {
        self.ops.check_mesh(mesh)?;
        for r in regions {
            if r.tets.is_empty() || r.tets.iter().any(|&t| t >= self.ops.n_tets) {
                return Err(shape(format!(
                    "region {}: empty or tet index out of range",
                    r.name
                )));
            }
        }
        let nt = self.n_terminals();
        let volt = |t: f64| -> Result<Vec<f64>, ElectrostaticError> {
            let v = waveform(t);
            if v.len() != nt {
                return Err(shape(format!(
                    "waveform length {} != terminal count {nt}",
                    v.len()
                )));
            }
            if let Some(x) = v.iter().find(|x| !x.is_finite()) {
                return Err(shape(format!("non-finite waveform value {x}")));
            }
            Ok(v)
        };
        let dt = self.opts.dt;
        let th = self.opts.theta;
        let ops = &self.ops;

        let v0 = volt(0.0)?;
        let mut phi = self.electrostatic_state(&v0)?;
        let mut steps = Vec::with_capacity(n_steps + 1);
        {
            let kphi = spmv(ops.k.as_ref(), &phi);
            let i_cond: Vec<f64> = ops
                .term_nodes
                .iter()
                .map(|n| Ops::sum_over(n, &kphi))
                .collect();
            let floating_current: Vec<f64> = ops
                .float_nodes
                .iter()
                .map(|n| Ops::sum_over(n, &kphi))
                .collect();
            steps.push(EqsStep {
                t: 0.0,
                v: v0,
                floating_phi: ops.float_nodes.iter().map(|n| phi[n[0] as usize]).collect(),
                i_disp: vec![0.0; nt],
                i_cond,
                floating_current,
                region_e: region_fields(mesh, &phi, regions),
                charge_residual: None,
            });
        }
        for s in 1..=n_steps {
            let t = s as f64 * dt;
            let v = volt(t)?;
            let w = ops.pinned(&v);
            let bphi = spmv(self.b_full.as_ref(), &phi);
            let aw = spmv(self.a_full.as_ref(), &w);
            let rhs_full: Vec<f64> = bphi.iter().zip(&aw).map(|(b, a)| b - a).collect();
            let u = lu_solve(&self.lu_a, &ops.reduce_vec(&rhs_full));
            check_finite(&u, "time step")?;
            let new = ops.expand(&u, &w);

            let k_new = spmv(ops.k.as_ref(), &new);
            let k_old = spmv(ops.k.as_ref(), &phi);
            let dphi: Vec<f64> = new.iter().zip(&phi).map(|(a, b)| (a - b) / dt).collect();
            let cd = spmv(ops.c.as_ref(), &dphi);
            let cond: Vec<f64> = k_new
                .iter()
                .zip(&k_old)
                .map(|(a, b)| th * a + (1.0 - th) * b)
                .collect();
            let i_cond: Vec<f64> = ops
                .term_nodes
                .iter()
                .map(|n| Ops::sum_over(n, &cond))
                .collect();
            let i_disp: Vec<f64> = ops
                .term_nodes
                .iter()
                .map(|n| Ops::sum_over(n, &cd))
                .collect();
            let floating_current: Vec<f64> = ops
                .float_nodes
                .iter()
                .map(|n| Ops::sum_over(n, &cond) + Ops::sum_over(n, &cd))
                .collect();
            let tot_terms: f64 = i_cond.iter().zip(&i_disp).map(|(a, b)| a + b).sum();
            let tot_float: f64 = floating_current.iter().sum();
            let scale = i_cond
                .iter()
                .zip(&i_disp)
                .map(|(a, b)| (a + b).abs())
                .fold(0.0_f64, f64::max)
                .max(f64::MIN_POSITIVE);
            steps.push(EqsStep {
                t,
                v,
                floating_phi: ops.float_nodes.iter().map(|n| new[n[0] as usize]).collect(),
                i_cond,
                i_disp,
                floating_current,
                region_e: region_fields(mesh, &new, regions),
                charge_residual: Some((tot_terms + tot_float).abs() / scale),
            });
            phi = new;
        }
        Ok(EqsHistory {
            terminal_names: ops.term_names.clone(),
            floating_names: ops.float_names.clone(),
            region_names: regions.iter().map(|r| r.name.clone()).collect(),
            steps,
            phi_final: phi,
        })
    }
}

/// Volume-averaged `E = −∇φ` per region.
fn region_fields(mesh: &TetMesh, phi: &[f64], regions: &[EqsRegion]) -> Vec<[f64; 3]> {
    regions
        .iter()
        .map(|r| {
            let mut acc = [0.0_f64; 3];
            let mut vol_sum = 0.0;
            for &t in &r.tets {
                let tet = &mesh.tets[t];
                let c = coords_of(mesh, tet);
                let vol = tet_p1_local(&c).2;
                let bg = tet_bary_grads(&c);
                for p in 0..4 {
                    for d in 0..3 {
                        acc[d] -= vol * phi[tet[p] as usize] * bg[p][d];
                    }
                }
                vol_sum += vol;
            }
            [acc[0] / vol_sum, acc[1] / vol_sum, acc[2] / vol_sum]
        })
        .collect()
}

/// Small-signal terminal admittance at one angular frequency.
#[derive(Debug, Clone)]
pub struct EqsAdmittance {
    /// Terminal names in row/column order.
    pub names: Vec<String>,
    /// Angular frequency (rad/s).
    pub omega: f64,
    /// `Re Y_ij` (S): `I_i = Σ_j Y_ij V_j`, current into the network.
    pub y_re: Vec<Vec<f64>>,
    /// `Im Y_ij` (S), `= ω C_ij` for a lossless dielectric.
    pub y_im: Vec<Vec<f64>>,
}

/// Terminal admittance matrix `Y = G + jωC` from `(K + jωC) φ = 0`.
///
/// Floating conductors carry zero net current. `omega` must be finite and
/// `≥ 0`.
///
/// # Errors
///
/// As [`assemble_eqs`] for the inputs; [`ElectrostaticError::Factorization`]
/// if the complex system is singular.
pub fn eqs_admittance(
    mesh: &TetMesh,
    eps: &[f64],
    sigma: &[f64],
    terminals: &[EqsTerminal],
    floating: &[EqsFloating],
    omega: f64,
) -> Result<EqsAdmittance, ElectrostaticError> {
    if !(omega.is_finite() && omega >= 0.0) {
        return Err(shape(format!("omega must be finite and >= 0, got {omega}")));
    }
    let ops = Ops::build(mesh, eps, sigma, terminals, floating)?;
    let m = ops.n_dof;
    let kr = ops.reduce_matrix(&ops.k)?;
    let cr = ops.reduce_matrix(&ops.c)?;
    let mut t: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for (mat, row0, col0, coef) in [
        (&kr, 0, 0, 1.0),
        (&kr, m, m, 1.0),
        (&cr, 0, m, -omega),
        (&cr, m, 0, omega),
    ] {
        let r = mat.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        for j in 0..m {
            for k in cp[j]..cp[j + 1] {
                t.push(Triplet::new(row0 + ri[k], col0 + j, coef * v[k]));
            }
        }
    }
    let big = Csc::try_new_from_triplets(2 * m, 2 * m, &t)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
    let lu = factor(&big)?;

    let n = terminals.len();
    let zero = vec![0.0; ops.n_nodes];
    let mut y_re = vec![vec![0.0; n]; n];
    let mut y_im = vec![vec![0.0; n]; n];
    for j in 0..n {
        let mut v = vec![0.0; n];
        v[j] = 1.0;
        let w = ops.pinned(&v);
        let mut rhs = Vec::with_capacity(2 * m);
        rhs.extend(ops.reduce_vec(&spmv(ops.k.as_ref(), &w)).iter().map(|x| -x));
        rhs.extend(
            ops.reduce_vec(&spmv(ops.c.as_ref(), &w))
                .iter()
                .map(|x| -omega * x),
        );
        let u = lu_solve(&lu, &rhs);
        check_finite(&u, "admittance solve")?;
        let re = ops.expand(&u[..m], &w);
        let im = ops.expand(&u[m..], &zero);
        let (k_re, k_im) = (spmv(ops.k.as_ref(), &re), spmv(ops.k.as_ref(), &im));
        let (c_re, c_im) = (spmv(ops.c.as_ref(), &re), spmv(ops.c.as_ref(), &im));
        for i in 0..n {
            let nodes = &ops.term_nodes[i];
            y_re[i][j] = Ops::sum_over(nodes, &k_re) - omega * Ops::sum_over(nodes, &c_im);
            y_im[i][j] = omega * Ops::sum_over(nodes, &c_re) + Ops::sum_over(nodes, &k_im);
        }
    }
    Ok(EqsAdmittance {
        names: ops.term_names,
        omega,
        y_re,
        y_im,
    })
}
