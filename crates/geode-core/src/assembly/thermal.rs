//! Steady heat conduction and Joule self-heating on a tetrahedral mesh
//! (issue #1556, piece 1 of #1555).
//!
//! Solves `∇·(k ∇T) = -q` with a per-tet thermal conductivity `k` (W/m/K),
//! a per-tet volumetric heat source `q` (W/m³), fixed-temperature (Dirichlet)
//! node sets and zero-flux natural boundaries, on the scalar P1 element
//! kernel [`tet_p1_local`] shared with [`crate::assembly::conduction`]
//! (`k` in place of `σ`; the mass part is unused).
//!
//! # Outputs
//!
//! [`solve_thermal`] returns the nodal temperature, the per-tet heat flux
//! `-k ∇T`, and an energy check: the heat leaving through the Dirichlet
//! nodes (nodal reactions) against the total source `Σ q_t V_t`.
//!
//! # Self-heating
//!
//! [`solve_self_heating`] couples the DC conduction solve of
//! [`crate::assembly::conduction`] (whose `joule_density` is the heat source)
//! with the thermal solve through a relaxed Picard iteration
//! `T ← T + ω (T_new − T)`, where `σ(T)` and `k(T)` are evaluated on the
//! per-tet mean temperature. The loop never runs away silently: it stops
//! with a typed [`PicardStatus`] on convergence, on the iteration cap, or
//! when the update size grows for several consecutive iterations (or the
//! temperature / material values stop being finite or exceed
//! [`PicardOptions::t_max`]).

#![allow(clippy::needless_range_loop)]

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::conduction::{ConductionOptions, ConductionTerminal, assemble_conduction};
use crate::assembly::electrostatic::{ElectrostaticError, tet_bary_grads, tet_p1_local};
use crate::mesh::TetMesh;

/// Lorenz number `L₀ = π²/3 (k_B/e)²` (W·Ω/K²) of the Wiedemann–Franz law.
pub const LORENZ_L0: f64 = 2.44e-8;

/// Wiedemann–Franz thermal conductivity `k = L₀ σ T` (W/m/K) of the
/// electronic contribution, for `σ` in S/m and `T` in kelvin.
pub fn wiedemann_franz_k(sigma: f64, temperature_k: f64) -> f64 {
    LORENZ_L0 * sigma * temperature_k
}

/// A fixed-temperature node set.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermalDirichlet {
    /// 0-based node indices (into `TetMesh.nodes`).
    pub nodes: Vec<u32>,
    /// Prescribed temperature at every node of the set.
    pub temperature: f64,
}

/// Solver tolerance of [`solve_thermal`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalOptions {
    /// Fail with [`ElectrostaticError::NotConverged`] if the relative
    /// residual `‖K u − b‖/‖b‖` of the reduced system exceeds this.
    pub residual_tol: f64,
}

impl Default for ThermalOptions {
    fn default() -> Self {
        Self {
            residual_tol: 1e-10,
        }
    }
}

/// One steady heat solve.
#[derive(Debug, Clone)]
pub struct ThermalSolution {
    /// Nodal temperature, length `n_nodes`.
    pub temperature: Vec<f64>,
    /// Heat flux `-k ∇T` (W/m²) per tet.
    pub flux: Vec<[f64; 3]>,
    /// `Σ q_t V_t` (W): total heat generated in the mesh.
    pub power_source: f64,
    /// Heat (W) leaving the mesh through the Dirichlet nodes, from the nodal
    /// reactions `b_n − (K T)_n` summed over them.
    pub boundary_heat: f64,
    /// `|boundary_heat − power_source| / max(|power_source|, tiny)`.
    pub energy_rel_mismatch: f64,
    /// Relative residual of the reduced system.
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

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// `∇T` on one tet from nodal values.
fn grad_of(c: &[[f64; 3]; 4], tet: &[u32; 4], t: &[f64]) -> [f64; 3] {
    let bg = tet_bary_grads(c);
    let mut g = [0.0_f64; 3];
    for p in 0..4 {
        let a = t[tet[p] as usize];
        for d in 0..3 {
            g[d] += a * bg[p][d];
        }
    }
    g
}

/// Solve `∇·(k∇T) = -q` with fixed-temperature node sets.
///
/// `k` (W/m/K, finite, `> 0`) and `q` (W/m³, finite) are per tet.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] for a bad `k`/`q` length or value,
/// no Dirichlet node (the problem is singular), a node index out of range,
/// a node pinned to two different temperatures or a non-finite temperature;
/// [`ElectrostaticError::Assembly`] / [`ElectrostaticError::Factorization`]
/// from faer; [`ElectrostaticError::NotConverged`] if the residual exceeds
/// [`ThermalOptions::residual_tol`] (e.g. a region cut off from every
/// Dirichlet node).
pub fn solve_thermal(
    mesh: &TetMesh,
    k: &[f64],
    q: &[f64],
    dirichlet: &[ThermalDirichlet],
    opts: ThermalOptions,
) -> Result<ThermalSolution, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if k.len() != n_tets || q.len() != n_tets {
        return Err(shape(format!(
            "k length {} / q length {} != tet count {n_tets}",
            k.len(),
            q.len()
        )));
    }
    if let Some(x) = k.iter().find(|x| !x.is_finite() || **x <= 0.0) {
        return Err(shape(format!("k must be finite and > 0, got {x}")));
    }
    if let Some(x) = q.iter().find(|x| !x.is_finite()) {
        return Err(shape(format!("non-finite heat source {x}")));
    }

    // Pinned temperatures.
    let mut fixed: Vec<Option<f64>> = vec![None; n_nodes];
    for set in dirichlet {
        if !set.temperature.is_finite() {
            return Err(shape(format!(
                "non-finite fixed temperature {}",
                set.temperature
            )));
        }
        for &n in &set.nodes {
            let n = n as usize;
            if n >= n_nodes {
                return Err(shape(format!("fixed node {n} >= node count {n_nodes}")));
            }
            if fixed[n].is_some_and(|t| t != set.temperature) {
                return Err(shape(format!("node {n} pinned to two temperatures")));
            }
            fixed[n] = Some(set.temperature);
        }
    }
    if fixed.iter().all(Option::is_none) {
        return Err(shape(
            "no fixed-temperature node: the heat problem is singular".into(),
        ));
    }

    // Reduced DOFs: every non-fixed node.
    let mut dof: Vec<Option<usize>> = vec![None; n_nodes];
    let mut n_dof = 0usize;
    for n in 0..n_nodes {
        if fixed[n].is_none() {
            dof[n] = Some(n_dof);
            n_dof += 1;
        }
    }

    // Assemble the reduced matrix, the load vector and the Dirichlet lift.
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
    let mut load = vec![0.0_f64; n_nodes];
    let mut b = vec![0.0_f64; n_dof];
    let mut power_source = 0.0;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let (kl, _m, vol) = tet_p1_local(&coords_of(mesh, tet));
        power_source += q[t] * vol;
        for p in 0..4 {
            let np = tet[p] as usize;
            load[np] += q[t] * vol / 4.0;
            let Some(dp) = dof[np] else { continue };
            b[dp] += q[t] * vol / 4.0;
            for r in 0..4 {
                let nr = tet[r] as usize;
                let v = k[t] * kl[p][r];
                match (dof[nr], fixed[nr]) {
                    (Some(dr), _) => trips.push(Triplet::new(dp, dr, v)),
                    (None, Some(tf)) => b[dp] -= v * tf,
                    (None, None) => unreachable!("node is free or fixed"),
                }
            }
        }
    }
    let k_red = SparseColMat::<usize, f64>::try_new_from_triplets(n_dof, n_dof, &trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    let mut u = vec![0.0_f64; n_dof];
    let mut residual = 0.0;
    if n_dof > 0 {
        use faer::linalg::solvers::Solve;
        let lu = k_red
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        let mut m: Mat<f64> = Mat::from_fn(n_dof, 1, |i, _| b[i]);
        lu.solve_in_place(m.as_mut());
        u = (0..n_dof).map(|i| m[(i, 0)]).collect();

        let r = k_red.as_ref();
        let (cp, ri, v) = (r.col_ptr(), r.row_idx(), r.val());
        let mut ku = vec![0.0; n_dof];
        for (j, &uj) in u.iter().enumerate() {
            for p in cp[j]..cp[j + 1] {
                ku[ri[p]] += v[p] * uj;
            }
        }
        let res: Vec<f64> = b.iter().zip(&ku).map(|(b, k)| b - k).collect();
        let bn = norm2(&b);
        residual = if bn > 0.0 {
            norm2(&res) / bn
        } else {
            norm2(&res)
        };
        if residual.is_nan() || residual > opts.residual_tol {
            return Err(ElectrostaticError::NotConverged(format!(
                "thermal relative residual {residual:.3e} > {:.3e}",
                opts.residual_tol
            )));
        }
    }

    let mut temperature = vec![0.0_f64; n_nodes];
    for n in 0..n_nodes {
        temperature[n] = match (dof[n], fixed[n]) {
            (Some(d), _) => u[d],
            (None, Some(tf)) => tf,
            (None, None) => unreachable!("node is free or fixed"),
        };
    }

    // Flux and nodal reactions (K T)_n of the full operator.
    let mut flux = Vec::with_capacity(n_tets);
    let mut kt = vec![0.0_f64; n_nodes];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let c = coords_of(mesh, tet);
        let g = grad_of(&c, tet, &temperature);
        flux.push([-k[t] * g[0], -k[t] * g[1], -k[t] * g[2]]);
        let kl = tet_p1_local(&c).0;
        for p in 0..4 {
            for r in 0..4 {
                kt[tet[p] as usize] += k[t] * kl[p][r] * temperature[tet[r] as usize];
            }
        }
    }
    let boundary_heat: f64 = (0..n_nodes)
        .filter(|&n| fixed[n].is_some())
        .map(|n| load[n] - kt[n])
        .sum();
    let energy_rel_mismatch =
        (boundary_heat - power_source).abs() / power_source.abs().max(f64::MIN_POSITIVE);

    Ok(ThermalSolution {
        temperature,
        flux,
        power_source,
        boundary_heat,
        energy_rel_mismatch,
        residual,
    })
}

/// Controls of the Picard self-heating loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PicardOptions {
    /// Relaxation `ω ∈ (0, 1]`: `T ← T + ω (T_new − T)`.
    pub relaxation: f64,
    /// Converged when `max|ΔT|` (the applied update) falls below this.
    pub tol: f64,
    /// Iteration cap.
    pub max_iter: usize,
    /// Declare [`PicardStatus::Runaway`] after this many consecutive
    /// iterations in which `max|ΔT|` grew.
    pub runaway_growth_iters: usize,
    /// Declare [`PicardStatus::Runaway`] if any nodal `T` exceeds this.
    pub t_max: f64,
    /// Tolerances of the inner conduction solves.
    pub conduction: ConductionOptions,
    /// Tolerances of the inner thermal solves.
    pub thermal: ThermalOptions,
}

impl Default for PicardOptions {
    fn default() -> Self {
        Self {
            relaxation: 1.0,
            tol: 1e-8,
            max_iter: 50,
            runaway_growth_iters: 4,
            t_max: 1e5,
            conduction: ConductionOptions::default(),
            thermal: ThermalOptions::default(),
        }
    }
}

/// How the Picard loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PicardStatus {
    /// `max|ΔT| < tol`.
    Converged,
    /// `max_iter` reached without convergence or runaway.
    NotConverged,
    /// Thermal runaway: the update grew for several consecutive iterations,
    /// or `T` / `σ` / `k` left the finite range or exceeded `t_max`.
    Runaway,
}

/// Result of [`solve_self_heating`]. On [`PicardStatus::Runaway`] and
/// [`PicardStatus::NotConverged`] the fields hold the last finite iterate
/// (`thermal` is `None` only if the very first thermal solve was not
/// reached).
#[derive(Debug, Clone)]
pub struct SelfHeatingResult {
    /// Why the loop stopped.
    pub status: PicardStatus,
    /// Iterations performed.
    pub iterations: usize,
    /// `max|ΔT|` of each iteration.
    pub history: Vec<f64>,
    /// Last nodal temperature.
    pub temperature: Vec<f64>,
    /// Last thermal solve (flux, energy balance).
    pub thermal: Option<ThermalSolution>,
    /// Last conduction solution (Joule density, currents).
    pub conduction: Option<crate::assembly::conduction::ConductionSolution>,
}

/// Relaxed Picard iteration of electro-thermal self-heating.
///
/// Starting from uniform `t_initial`, each pass evaluates the per-tet mean
/// temperature, `σ = sigma_of_t(tet, T)` and `k = k_of_t(tet, T, σ)`, solves
/// the conduction problem at `voltages`, feeds its `joule_density` to
/// [`solve_thermal`] and relaxes the temperature.
///
/// # Errors
///
/// Input and solver errors of [`assemble_conduction`] / [`solve_thermal`],
/// and `relaxation ∉ (0, 1]`. Thermal runaway and non-convergence are
/// reported through [`SelfHeatingResult::status`], not as errors.
#[allow(clippy::too_many_arguments)]
pub fn solve_self_heating(
    mesh: &TetMesh,
    terminals: &[ConductionTerminal],
    voltages: &[f64],
    dirichlet: &[ThermalDirichlet],
    t_initial: f64,
    sigma_of_t: impl Fn(usize, f64) -> f64,
    k_of_t: impl Fn(usize, f64, f64) -> f64,
    opts: PicardOptions,
) -> Result<SelfHeatingResult, ElectrostaticError> {
    if !(opts.relaxation > 0.0 && opts.relaxation <= 1.0) {
        return Err(shape(format!(
            "relaxation must be in (0, 1], got {}",
            opts.relaxation
        )));
    }
    let n_nodes = mesh.n_nodes();
    let mut t_nodal = vec![t_initial; n_nodes];
    for set in dirichlet {
        for &n in &set.nodes {
            if (n as usize) < n_nodes {
                t_nodal[n as usize] = set.temperature;
            }
        }
    }
    let mut out = SelfHeatingResult {
        status: PicardStatus::NotConverged,
        iterations: 0,
        history: Vec::new(),
        temperature: t_nodal.clone(),
        thermal: None,
        conduction: None,
    };
    let mut growth = 0usize;
    let mut prev = f64::INFINITY;
    for it in 0..opts.max_iter {
        let (mut sigma, mut kk) = (
            Vec::with_capacity(mesh.n_tets()),
            Vec::with_capacity(mesh.n_tets()),
        );
        for (t, tet) in mesh.tets.iter().enumerate() {
            let tm = tet.iter().map(|&n| t_nodal[n as usize]).sum::<f64>() / 4.0;
            let s = sigma_of_t(t, tm);
            sigma.push(s);
            kk.push(k_of_t(t, tm, s));
        }
        if sigma.iter().chain(&kk).any(|x| !x.is_finite()) {
            out.status = PicardStatus::Runaway;
            return Ok(out);
        }
        let sys = assemble_conduction(mesh, &sigma, terminals, opts.conduction)?;
        let cond = sys.solve(mesh, voltages)?;
        let th = solve_thermal(mesh, &kk, &cond.joule_density, dirichlet, opts.thermal)?;

        let mut dmax = 0.0_f64;
        for n in 0..n_nodes {
            let d = opts.relaxation * (th.temperature[n] - t_nodal[n]);
            t_nodal[n] += d;
            dmax = dmax.max(d.abs());
        }
        out.iterations = it + 1;
        out.history.push(dmax);
        if !dmax.is_finite() || t_nodal.iter().any(|t| !t.is_finite() || *t > opts.t_max) {
            out.status = PicardStatus::Runaway;
            return Ok(out);
        }
        out.temperature = t_nodal.clone();
        out.thermal = Some(th);
        out.conduction = Some(cond);
        if dmax < opts.tol {
            out.status = PicardStatus::Converged;
            return Ok(out);
        }
        growth = if dmax > prev { growth + 1 } else { 0 };
        if growth >= opts.runaway_growth_iters {
            out.status = PicardStatus::Runaway;
            return Ok(out);
        }
        prev = dmax;
    }
    Ok(out)
}
