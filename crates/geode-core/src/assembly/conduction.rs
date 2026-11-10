//! Steady DC **current conduction** on a tetrahedral mesh (issue #1476).
//!
//! Solves `∇·(σ ∇φ) = 0` with a per-tet conductivity `σ` (S/m), Dirichlet
//! terminals and insulating (natural, `σ ∂φ/∂n = 0`) boundaries on the same
//! scalar P1 element kernel as [`crate::assembly::electrostatic`]
//! ([`tet_p1_local`]), with `σ` in place of `ε₀ε_r`.
//!
//! # Outputs
//!
//! * [`ConductionSystem::conductance_matrix`] — the N×N terminal conductance
//!   matrix `G` (S): N solves sharing one LU; symmetric; rows sum to zero.
//! * [`ConductionSystem::solve`] — one operating point: potential, per-tet
//!   current density `J = σE`, Joule density `σ|∇φ|²`, terminal currents,
//!   total dissipated power and its balance against `Σ V_i I_i`, and the
//!   solver residual.
//!
//! # Conditioning for 12+ decades of contrast
//!
//! `σ` is normalised by its maximum, the reduced matrix is Jacobi-scaled
//! `D^-1/2 K D^-1/2` before the sparse LU, and the **unscaled** relative
//! residual `‖K u − b‖/‖b‖` is evaluated; a few steps of iterative
//! refinement run only if it exceeds [`ConductionOptions::refine_above`], and
//! the solve fails with [`ElectrostaticError::NotConverged`] if it still
//! exceeds [`ConductionOptions::residual_tol`].
//!
//! Quantities that would suffer cancellation when `φ` is nearly constant over
//! a highly conducting region are evaluated **element-wise from gradients**:
//! `G_ij = Σ_t σ_t V_t ∇φ_i·∇φ_j` (symmetric by construction) rather than
//! from `φᵀKφ`. The terminal currents of [`ConductionSolution`] are nodal
//! reactions `(Kφ)_n`, an independent quantity for the power-balance check;
//! these do cancel, so for contrasts above ~8 decades their accuracy (and
//! hence the balance) degrades while `G` stays accurate.
//!
//! # Lumped external resistance
//!
//! A terminal with `r_ext = Some(R)` becomes **one equipotential unknown**
//! `u_i` tied to its applied voltage `V_i` through `g = 1/R`. `r_ext = None`
//! keeps the Dirichlet pin. `G` then describes the network including the
//! series resistors, with terminal `i` at the external side of `R`.
//!
//! # Zero conductivity
//!
//! A node touched only by `σ = 0` tets carries no current and is dropped
//! from the unknowns (`φ = 0` is reported there). A conducting island joined
//! to no terminal is singular and fails to factor.

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::electrostatic::{ElectrostaticError, tet_bary_grads, tet_p1_local};
use crate::mesh::TetMesh;

/// A conduction terminal: a named node set, either pinned (`r_ext = None`)
/// or held through a lumped resistance (Ω).
#[derive(Debug, Clone, PartialEq)]
pub struct ConductionTerminal {
    /// Terminal name (for the named accessors on [`ConductanceMatrix`]).
    pub name: String,
    /// 0-based node indices (into `TetMesh.nodes`); must be non-empty and
    /// disjoint from every other terminal.
    pub nodes: Vec<u32>,
    /// Lumped series resistance (Ω, finite and `> 0`) to the applied voltage.
    pub r_ext: Option<f64>,
}

/// Solver tolerances for [`assemble_conduction`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConductionOptions {
    /// Run iterative refinement while the relative residual exceeds this.
    pub refine_above: f64,
    /// Maximum refinement steps.
    pub max_refine: usize,
    /// Fail with `NotConverged` if the final relative residual exceeds this.
    pub residual_tol: f64,
}

impl Default for ConductionOptions {
    fn default() -> Self {
        Self {
            refine_above: 1e-13,
            max_refine: 5,
            residual_tol: 1e-10,
        }
    }
}

/// An assembled, factored conduction problem.
#[derive(Debug)]
pub struct ConductionSystem {
    n_nodes: usize,
    n_tets: usize,
    names: Vec<String>,
    nodes: Vec<Vec<u32>>,
    /// Normalised `1/R` (conductance, in units of `σ_max·m`) per terminal.
    g_hat: Vec<Option<f64>>,
    /// Reduced DOF of a node: `None` for pinned or inactive nodes.
    dof_of_node: Vec<Option<usize>>,
    /// Reduced DOF of each r_ext terminal.
    dof_of_terminal: Vec<Option<usize>>,
    sigma: Vec<f64>,
    sigma_max: f64,
    /// Full normalised stiffness (`σ/σ_max`), `n_nodes × n_nodes`.
    k_full: SparseColMat<usize, f64>,
    /// Reduced normalised matrix (with `g` on r_ext diagonals), unscaled.
    k_red: SparseColMat<usize, f64>,
    scale: Vec<f64>,
    lu: faer::sparse::linalg::solvers::Lu<usize, f64>,
    n_dof: usize,
    opts: ConductionOptions,
}

/// Terminal conductance matrix `G` (S): `I_i = Σ_j G_ij V_j`.
#[derive(Debug, Clone)]
pub struct ConductanceMatrix {
    /// Terminal names in row/column order.
    pub names: Vec<String>,
    /// `G[i][j]` (S).
    pub g: Vec<Vec<f64>>,
    /// Worst relative residual over the N solves.
    pub max_residual: f64,
}

impl ConductanceMatrix {
    /// Index of a named terminal.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// `max |G_ij − G_ji| / max(|G_ij|, |G_ji|)` over off-diagonal pairs.
    pub fn max_rel_asymmetry(&self) -> f64 {
        let n = self.names.len();
        let mut worst = 0.0_f64;
        for i in 0..n {
            for j in (i + 1)..n {
                let (a, b) = (self.g[i][j], self.g[j][i]);
                let d = a.abs().max(b.abs());
                if d > 0.0 {
                    worst = worst.max((a - b).abs() / d);
                }
            }
        }
        worst
    }

    /// `max_i |Σ_j G_ij| / G_ii`: zero when no terminal drains to ground.
    pub fn max_rel_row_sum(&self) -> f64 {
        self.g
            .iter()
            .enumerate()
            .map(|(i, r)| r.iter().sum::<f64>().abs() / self.g[i][i].abs().max(f64::MIN_POSITIVE))
            .fold(0.0, f64::max)
    }
}

/// One operating point of the conduction problem.
#[derive(Debug, Clone)]
pub struct ConductionSolution {
    /// Nodal potential (V), length `n_nodes`. An r_ext terminal's nodes carry
    /// its internal potential `u_i`.
    pub phi: Vec<f64>,
    /// Current density `J = σE` (A/m²) per tet.
    pub j: Vec<[f64; 3]>,
    /// Joule heat density `σ|∇φ|²` (W/m³) per tet.
    pub joule_density: Vec<f64>,
    /// Current (A) entering the network at each terminal's external side.
    pub terminal_current: Vec<f64>,
    /// `Σ_t σ_t |∇φ|² V_t` (W): dissipation in the mesh.
    pub power_volume: f64,
    /// `Σ g (V_i − u_i)²` (W): dissipation in the lumped resistors.
    pub power_resistors: f64,
    /// `Σ V_i I_i` (W) from the terminal currents.
    pub power_terminals: f64,
    /// `|P_volume + P_resistors − Σ V_i I_i| / max(|Σ V_i I_i|, tiny)`.
    pub power_rel_mismatch: f64,
    /// Relative residual `‖K u − b‖/‖b‖` of the unscaled reduced system.
    pub residual: f64,
}

fn shape(msg: String) -> ElectrostaticError {
    ElectrostaticError::ShapeMismatch(msg)
}

fn coords_of(mesh: &TetMesh, tet: &[u32; 4]) -> [[f64; 3]; 4] {
    [
        mesh.nodes[tet[0] as usize],
        mesh.nodes[tet[1] as usize],
        mesh.nodes[tet[2] as usize],
        mesh.nodes[tet[3] as usize],
    ]
}

/// `y = A x` for a CSC matrix.
fn csc_matvec(a: &SparseColMat<usize, f64>, x: &[f64]) -> Vec<f64> {
    let r = a.as_ref();
    let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
    let mut y = vec![0.0; r.nrows()];
    for (j, &xj) in x.iter().enumerate() {
        for k in cp[j]..cp[j + 1] {
            y[ri[k]] += v[k] * xj;
        }
    }
    y
}

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Assemble and factor the conduction problem.
///
/// `sigma` is the per-tet conductivity (S/m, finite, `≥ 0`, not all zero).
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] for a bad `sigma`, a node index out
/// of range, overlapping terminals or an invalid `r_ext`;
/// [`ElectrostaticError::EmptyElectrode`] for a terminal with no nodes;
/// [`ElectrostaticError::Assembly`] / [`ElectrostaticError::Factorization`]
/// from faer.
pub fn assemble_conduction(
    mesh: &TetMesh,
    sigma: &[f64],
    terminals: &[ConductionTerminal],
    opts: ConductionOptions,
) -> Result<ConductionSystem, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if sigma.len() != n_tets {
        return Err(shape(format!(
            "sigma length {} != tet count {n_tets}",
            sigma.len()
        )));
    }
    if let Some(s) = sigma.iter().find(|s| !s.is_finite() || **s < 0.0) {
        return Err(shape(format!("sigma must be finite and >= 0, got {s}")));
    }
    let sigma_max = sigma.iter().copied().fold(0.0, f64::max);
    if sigma_max <= 0.0 {
        return Err(shape("sigma is zero everywhere".into()));
    }
    if terminals.is_empty() {
        return Err(shape("no terminals".into()));
    }

    // Terminal bookkeeping: owner of each node.
    let mut owner: Vec<Option<usize>> = vec![None; n_nodes];
    for (ti, t) in terminals.iter().enumerate() {
        if t.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(t.name.clone()));
        }
        if let Some(r) = t.r_ext
            && !(r.is_finite() && r > 0.0)
        {
            return Err(shape(format!(
                "terminal {}: r_ext must be finite and > 0, got {r}",
                t.name
            )));
        }
        for &n in &t.nodes {
            let n = n as usize;
            if n >= n_nodes {
                return Err(shape(format!(
                    "terminal {}: node {n} >= node count {n_nodes}",
                    t.name
                )));
            }
            if owner[n].is_some_and(|o| o != ti) {
                return Err(shape(format!(
                    "node {n} belongs to more than one terminal ({})",
                    t.name
                )));
            }
            owner[n] = Some(ti);
        }
    }

    // Full normalised stiffness.
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
    let mut active = vec![false; n_nodes];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let s = sigma[t] / sigma_max;
        if s == 0.0 {
            continue;
        }
        let (k_local, _m, _v) = tet_p1_local(&coords_of(mesh, tet));
        for p in 0..4 {
            active[tet[p] as usize] = true;
            for q in 0..4 {
                trips.push(Triplet::new(
                    tet[p] as usize,
                    tet[q] as usize,
                    s * k_local[p][q],
                ));
            }
        }
    }
    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_nodes, n_nodes, &trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    // Reduced DOFs: active non-terminal nodes are unknowns; Dirichlet
    // terminal nodes are pinned; r_ext terminals share one DOF.
    let mut dof_of_node: Vec<Option<usize>> = vec![None; n_nodes];
    let mut dof_of_terminal: Vec<Option<usize>> = vec![None; terminals.len()];
    let mut n_dof = 0usize;
    for (ti, t) in terminals.iter().enumerate() {
        if t.r_ext.is_some() {
            dof_of_terminal[ti] = Some(n_dof);
            for &n in &t.nodes {
                dof_of_node[n as usize] = Some(n_dof);
            }
            n_dof += 1;
        }
    }
    for n in 0..n_nodes {
        if owner[n].is_none() && active[n] {
            dof_of_node[n] = Some(n_dof);
            n_dof += 1;
        }
    }

    let g_hat: Vec<Option<f64>> = terminals
        .iter()
        .map(|t| t.r_ext.map(|r| 1.0 / (r * sigma_max)))
        .collect();

    let mut red: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(trips.len());
    let kr = k_full.as_ref();
    let (cp, ri, kv) = (kr.col_ptr(), kr.row_idx(), kr.val());
    for j in 0..n_nodes {
        let Some(dj) = dof_of_node[j] else { continue };
        for k in cp[j]..cp[j + 1] {
            if let Some(di) = dof_of_node[ri[k]] {
                red.push(Triplet::new(di, dj, kv[k]));
            }
        }
    }
    for (ti, g) in g_hat.iter().enumerate() {
        if let (Some(g), Some(d)) = (g, dof_of_terminal[ti]) {
            red.push(Triplet::new(d, d, *g));
        }
    }
    let k_red = SparseColMat::<usize, f64>::try_new_from_triplets(n_dof, n_dof, &red)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    // Jacobi scaling D^-1/2 K D^-1/2 and LU of the scaled matrix.
    let mut diag = vec![0.0_f64; n_dof];
    {
        let r = k_red.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        for j in 0..n_dof {
            for k in cp[j]..cp[j + 1] {
                if ri[k] == j {
                    diag[j] += v[k];
                }
            }
        }
    }
    if let Some(d) = diag.iter().find(|d| d.is_nan() || **d <= 0.0) {
        return Err(ElectrostaticError::Factorization(format!(
            "non-positive reduced diagonal {d}"
        )));
    }
    let scale: Vec<f64> = diag.iter().map(|d| 1.0 / d.sqrt()).collect();
    let mut scaled: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(red.len());
    {
        let r = k_red.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        for j in 0..n_dof {
            for k in cp[j]..cp[j + 1] {
                scaled.push(Triplet::new(ri[k], j, v[k] * scale[ri[k]] * scale[j]));
            }
        }
    }
    let k_scaled = SparseColMat::<usize, f64>::try_new_from_triplets(n_dof, n_dof, &scaled)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
    let lu = k_scaled
        .as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;

    Ok(ConductionSystem {
        n_nodes,
        n_tets,
        names: terminals.iter().map(|t| t.name.clone()).collect(),
        nodes: terminals.iter().map(|t| t.nodes.clone()).collect(),
        g_hat,
        dof_of_node,
        dof_of_terminal,
        sigma: sigma.to_vec(),
        sigma_max,
        k_full,
        k_red,
        scale,
        lu,
        n_dof,
        opts,
    })
}

impl ConductionSystem {
    /// Number of terminals.
    pub fn n_terminals(&self) -> usize {
        self.names.len()
    }

    /// Solve the reduced system for `b`; returns `(u, relative residual)`.
    fn solve_reduced(&self, b: &[f64]) -> Result<(Vec<f64>, f64), ElectrostaticError> {
        use faer::linalg::solvers::Solve;
        let bn = norm2(b);
        if bn == 0.0 {
            return Ok((vec![0.0; self.n_dof], 0.0));
        }
        let apply = |rhs: &[f64]| -> Vec<f64> {
            let mut m: Mat<f64> = Mat::from_fn(self.n_dof, 1, |i, _| rhs[i] * self.scale[i]);
            self.lu.solve_in_place(m.as_mut());
            (0..self.n_dof).map(|i| m[(i, 0)] * self.scale[i]).collect()
        };
        let mut u = apply(b);
        let mut rel = 0.0;
        for step in 0..=self.opts.max_refine {
            let ku = csc_matvec(&self.k_red, &u);
            let r: Vec<f64> = b.iter().zip(&ku).map(|(b, k)| b - k).collect();
            rel = norm2(&r) / bn;
            if rel <= self.opts.refine_above || step == self.opts.max_refine {
                break;
            }
            for (ui, di) in u.iter_mut().zip(apply(&r)) {
                *ui += di;
            }
        }
        if rel.is_nan() || rel > self.opts.residual_tol {
            return Err(ElectrostaticError::NotConverged(format!(
                "conduction relative residual {rel:.3e} > {:.3e}",
                self.opts.residual_tol
            )));
        }
        Ok((u, rel))
    }

    /// Full potential, internal terminal potentials and residual for
    /// applied terminal voltages.
    fn potential(&self, v: &[f64]) -> Result<(Vec<f64>, f64), ElectrostaticError> {
        if v.len() != self.n_terminals() {
            return Err(shape(format!(
                "voltages length {} != terminal count {}",
                v.len(),
                self.n_terminals()
            )));
        }
        if let Some(x) = v.iter().find(|x| !x.is_finite()) {
            return Err(shape(format!("non-finite terminal voltage {x}")));
        }
        let mut phi = vec![0.0_f64; self.n_nodes];
        for (ti, nodes) in self.nodes.iter().enumerate() {
            if self.g_hat[ti].is_none() {
                for &n in nodes {
                    phi[n as usize] = v[ti];
                }
            }
        }
        // b = -K[free, pinned] V_pinned + g V on r_ext DOFs.
        let mut b = vec![0.0_f64; self.n_dof];
        let kr = self.k_full.as_ref();
        let (cp, ri, kv) = (kr.col_ptr(), kr.row_idx(), kr.val());
        for (ti, nodes) in self.nodes.iter().enumerate() {
            if self.g_hat[ti].is_some() || v[ti] == 0.0 {
                continue;
            }
            for &n in nodes {
                let j = n as usize;
                for k in cp[j]..cp[j + 1] {
                    if let Some(di) = self.dof_of_node[ri[k]] {
                        b[di] -= kv[k] * v[ti];
                    }
                }
            }
        }
        for (ti, g) in self.g_hat.iter().enumerate() {
            if let (Some(g), Some(d)) = (g, self.dof_of_terminal[ti]) {
                b[d] += g * v[ti];
            }
        }
        let (u, rel) = self.solve_reduced(&b)?;
        for (n, d) in self.dof_of_node.iter().enumerate() {
            if let Some(d) = d {
                phi[n] = u[*d];
            }
        }
        Ok((phi, rel))
    }

    /// `u_i` of an r_ext terminal in a full potential.
    fn terminal_potential(&self, phi: &[f64], ti: usize) -> f64 {
        phi[self.nodes[ti][0] as usize]
    }

    /// Solve for the applied terminal voltages `v` (V, one per terminal).
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] on a length mismatch or
    /// non-finite voltage; [`ElectrostaticError::NotConverged`] if the
    /// residual exceeds [`ConductionOptions::residual_tol`].
    pub fn solve(
        &self,
        mesh: &TetMesh,
        v: &[f64],
    ) -> Result<ConductionSolution, ElectrostaticError> {
        if mesh.n_nodes() != self.n_nodes || mesh.n_tets() != self.n_tets {
            return Err(shape("mesh differs from the assembled system".into()));
        }
        let (phi, residual) = self.potential(v)?;

        let mut j = Vec::with_capacity(self.n_tets);
        let mut joule = Vec::with_capacity(self.n_tets);
        let mut power_volume = 0.0;
        for (t, tet) in mesh.tets.iter().enumerate() {
            let c = coords_of(mesh, tet);
            let vol = tet_p1_local(&c).2;
            let g = grad_of(&c, tet, &phi);
            let s = self.sigma[t];
            j.push([-s * g[0], -s * g[1], -s * g[2]]);
            let q = s * (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]);
            joule.push(q);
            power_volume += q * vol;
        }

        // Terminal currents: nodal reactions (K phi) of the physical K.
        let kphi = csc_matvec(&self.k_full, &phi);
        let mut terminal_current = Vec::with_capacity(self.n_terminals());
        let mut power_resistors = 0.0;
        for (ti, nodes) in self.nodes.iter().enumerate() {
            if let Some(g) = self.g_hat[ti] {
                let g_phys = g * self.sigma_max;
                let d = v[ti] - self.terminal_potential(&phi, ti);
                terminal_current.push(g_phys * d);
                power_resistors += g_phys * d * d;
            } else {
                let sum: f64 = nodes.iter().map(|&n| kphi[n as usize]).sum();
                terminal_current.push(sum * self.sigma_max);
            }
        }
        let power_terminals: f64 = v.iter().zip(&terminal_current).map(|(a, b)| a * b).sum();
        let power_rel_mismatch = (power_volume + power_resistors - power_terminals).abs()
            / power_terminals.abs().max(f64::MIN_POSITIVE);

        Ok(ConductionSolution {
            phi,
            j,
            joule_density: joule,
            terminal_current,
            power_volume,
            power_resistors,
            power_terminals,
            power_rel_mismatch,
            residual,
        })
    }

    /// The N×N terminal conductance matrix from N unit-voltage solves.
    ///
    /// `G_ij = Σ_t σ_t V_t ∇φ_i·∇φ_j + Σ_k g_k (δ_ki − u_k^i)(δ_kj − u_k^j)`
    /// (the second term only for r_ext terminals): symmetric by construction
    /// and free of the cancellation that `φᵀKφ` suffers at high contrast.
    ///
    /// # Errors
    ///
    /// As [`solve`](Self::solve).
    pub fn conductance_matrix(
        &self,
        mesh: &TetMesh,
    ) -> Result<ConductanceMatrix, ElectrostaticError> {
        if mesh.n_nodes() != self.n_nodes || mesh.n_tets() != self.n_tets {
            return Err(shape("mesh differs from the assembled system".into()));
        }
        let n = self.n_terminals();
        let mut grads: Vec<Vec<[f64; 3]>> = Vec::with_capacity(n);
        let mut dv: Vec<Vec<f64>> = Vec::with_capacity(n); // resistor drops
        let mut max_residual = 0.0_f64;
        for i in 0..n {
            let mut v = vec![0.0; n];
            v[i] = 1.0;
            let (phi, rel) = self.potential(&v)?;
            max_residual = max_residual.max(rel);
            grads.push(
                mesh.tets
                    .iter()
                    .map(|tet| grad_of(&coords_of(mesh, tet), tet, &phi))
                    .collect(),
            );
            dv.push(
                (0..n)
                    .map(|k| {
                        if self.g_hat[k].is_some() {
                            v[k] - self.terminal_potential(&phi, k)
                        } else {
                            0.0
                        }
                    })
                    .collect(),
            );
        }
        let wv: Vec<f64> = mesh
            .tets
            .iter()
            .enumerate()
            .map(|(t, tet)| self.sigma[t] * tet_p1_local(&coords_of(mesh, tet)).2)
            .collect();
        let mut g = vec![vec![0.0_f64; n]; n];
        for i in 0..n {
            for j in i..n {
                let mut s = 0.0;
                for (t, w) in wv.iter().enumerate() {
                    let (a, b) = (grads[i][t], grads[j][t]);
                    s += w * (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]);
                }
                for (k, gk) in self.g_hat.iter().enumerate() {
                    if let Some(gk) = gk {
                        s += gk * self.sigma_max * dv[i][k] * dv[j][k];
                    }
                }
                g[i][j] = s;
                g[j][i] = s;
            }
        }
        Ok(ConductanceMatrix {
            names: self.names.clone(),
            g,
            max_residual,
        })
    }
}

/// `∇φ` on one tet from nodal values.
fn grad_of(c: &[[f64; 3]; 4], tet: &[u32; 4], phi: &[f64]) -> [f64; 3] {
    let bg = tet_bary_grads(c);
    let mut g = [0.0_f64; 3];
    for p in 0..4 {
        let a = phi[tet[p] as usize];
        for d in 0..3 {
            g[d] += a * bg[p][d];
        }
    }
    g
}
