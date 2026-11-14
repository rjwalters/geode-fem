//! Linear **thermo-elastic** stress on a tetrahedral mesh (issue #1557,
//! piece 2 of #1555).
//!
//! Vector-P1 (constant-strain tet) isotropic elasticity driven by a thermal
//! eigenstrain `ε_th = α (T − T_ref) I` from a nodal temperature field `T`
//! (for example the heat-conduction solution of
//! [`crate::assembly::conduction`], or any analytic field: `T` is just a
//! `&[f64]` of length `n_nodes`).
//!
//! # Formulation
//!
//! DOFs are ordered `3·node + component`. Strains and stresses use Voigt
//! order `(xx, yy, zz, yz, xz, xy)` with engineering shear strain, and the
//! isotropic stiffness `C(E, ν)` has `λ = Eν/((1+ν)(1−2ν))`, `μ = E/(2(1+ν))`.
//! For a tet with volume `V` and strain-displacement matrix `B` (6×12,
//! built from the barycentric gradients):
//!
//! * `Ke = V Bᵀ C B`,
//! * thermal load `fe = V Bᵀ C ε_th`, with `ε_th = α (T̄ − T_ref)(1,1,1,0,0,0)`
//!   and `T̄` the mean of the four nodal values,
//! * recovered stress `σ = C (B u − ε_th)` (constant per tet), hydrostatic
//!   stress `tr(σ)/3` (the quantity the electromigration coupling of #1558
//!   consumes; tension positive).
//!
//! Displacement constraints are applied by row elimination: constrained
//! DOFs are removed from the unknowns and their (possibly non-zero) values
//! move to the right-hand side. The reduced system is solved with a sparse
//! LU. Rigid-body modes must be removed by the constraints. An empty
//! constraint set is rejected, but a partial under-constraint is **not**
//! reliably detected (round-off pivots can mask the singularity), so the
//! caller owns it; non-finite results are reported as a factorization error.
//!
//! # Exactness
//!
//! The P1 tet carries a constant strain, so a uniform `ΔT` with
//! displacement constraints that make the exact solution a linear field
//! (e.g. a bar held at both ends) is reproduced to round-off.
//!
//! # References
//!
//! * S. Timoshenko, "Analysis of bi-metal thermostats", J. Opt. Soc. Am.
//!   11, 233 (1925) — bimaterial strip curvature, used as the bending
//!   validation in `tests/thermoelastic_validation.rs`.
#![allow(clippy::needless_range_loop)]

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::electrostatic::{ElectrostaticError, tet_bary_grads, tet_p1_local};
use crate::mesh::TetMesh;

/// Isotropic, temperature-independent material of one tet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermoelasticMaterial {
    /// Young's modulus `E` (Pa), finite and `> 0`.
    pub youngs_modulus: f64,
    /// Poisson's ratio `ν`, in `(-1, 0.5)`.
    pub poisson_ratio: f64,
    /// Linear thermal expansion coefficient `α` (1/K), finite.
    pub alpha: f64,
}

/// A displacement constraint: the listed components of the listed nodes are
/// prescribed to `value` (m). Later constraints override earlier ones on a
/// shared DOF.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplacementConstraint {
    /// 0-based node indices (into `TetMesh.nodes`).
    pub nodes: Vec<u32>,
    /// Which of `(x, y, z)` are prescribed.
    pub components: [bool; 3],
    /// Prescribed displacement (m); `0.0` for fixed / symmetry planes.
    pub value: f64,
}

/// Result of [`solve_thermoelastic`].
#[derive(Debug, Clone)]
pub struct ThermoelasticSolution {
    /// Nodal displacement (m), length `n_nodes`.
    pub displacement: Vec<[f64; 3]>,
    /// Per-tet stress (Pa), Voigt `(xx, yy, zz, yz, xz, xy)`.
    pub stress: Vec<[f64; 6]>,
    /// Per-tet hydrostatic stress `tr(σ)/3` (Pa, tension positive).
    pub hydrostatic_stress: Vec<f64>,
}

fn shape(msg: String) -> ElectrostaticError {
    ElectrostaticError::ShapeMismatch(msg)
}

/// Isotropic stiffness `C(E, ν)` in Voigt order.
fn stiffness(m: &ThermoelasticMaterial) -> [[f64; 6]; 6] {
    let (e, nu) = (m.youngs_modulus, m.poisson_ratio);
    let lam = e * nu / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let mu = e / (2.0 * (1.0 + nu));
    let mut c = [[0.0; 6]; 6];
    for i in 0..3 {
        for j in 0..3 {
            c[i][j] = lam;
        }
        c[i][i] = lam + 2.0 * mu;
        c[i + 3][i + 3] = mu;
    }
    c
}

/// Strain-displacement matrix `B` (6×12) from the barycentric gradients.
fn strain_b(coords: &[[f64; 3]; 4]) -> [[f64; 12]; 6] {
    let g = tet_bary_grads(coords);
    let mut b = [[0.0; 12]; 6];
    for a in 0..4 {
        let [gx, gy, gz] = g[a];
        let c = 3 * a;
        b[0][c] = gx;
        b[1][c + 1] = gy;
        b[2][c + 2] = gz;
        b[3][c + 1] = gz;
        b[3][c + 2] = gy;
        b[4][c] = gz;
        b[4][c + 2] = gx;
        b[5][c] = gy;
        b[5][c + 1] = gx;
    }
    b
}

fn mat_vec6(c: &[[f64; 6]; 6], v: &[f64; 6]) -> [f64; 6] {
    let mut r = [0.0; 6];
    for i in 0..6 {
        for j in 0..6 {
            r[i] += c[i][j] * v[j];
        }
    }
    r
}

/// Solve the one-way-coupled thermo-elastic problem for the nodal
/// temperature `temperature` relative to the stress-free `t_ref`.
///
/// `materials` has one entry per tet. See the module docs for conventions.
pub fn solve_thermoelastic(
    mesh: &TetMesh,
    materials: &[ThermoelasticMaterial],
    temperature: &[f64],
    t_ref: f64,
    constraints: &[DisplacementConstraint],
) -> Result<ThermoelasticSolution, ElectrostaticError> {
    use faer::linalg::solvers::Solve;

    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if materials.len() != n_tets {
        return Err(shape(format!(
            "materials has {} entries, mesh has {n_tets} tets",
            materials.len()
        )));
    }
    if temperature.len() != n_nodes {
        return Err(shape(format!(
            "temperature has {} entries, mesh has {n_nodes} nodes",
            temperature.len()
        )));
    }
    if !t_ref.is_finite() || temperature.iter().any(|t| !t.is_finite()) {
        return Err(shape("temperature / t_ref must be finite".into()));
    }
    for (i, m) in materials.iter().enumerate() {
        let ok = m.youngs_modulus.is_finite()
            && m.youngs_modulus > 0.0
            && m.poisson_ratio > -1.0
            && m.poisson_ratio < 0.5
            && m.alpha.is_finite();
        if !ok {
            return Err(shape(format!("invalid material for tet {i}: {m:?}")));
        }
    }

    let n_dof = 3 * n_nodes;
    let mut prescribed: Vec<Option<f64>> = vec![None; n_dof];
    for c in constraints {
        for &n in &c.nodes {
            if n as usize >= n_nodes {
                return Err(shape(format!("constraint node {n} out of range")));
            }
            for k in 0..3 {
                if c.components[k] {
                    prescribed[3 * n as usize + k] = Some(c.value);
                }
            }
        }
    }
    if prescribed.iter().all(Option::is_none) {
        return Err(shape(
            "no displacement constraints: rigid-body modes are unconstrained".into(),
        ));
    }
    let mut free_of: Vec<Option<usize>> = vec![None; n_dof];
    let mut n_free = 0;
    for d in 0..n_dof {
        if prescribed[d].is_none() {
            free_of[d] = Some(n_free);
            n_free += 1;
        }
    }

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 144);
    let mut rhs = vec![0.0; n_free];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (_, _, vol) = tet_p1_local(&coords);
        let b = strain_b(&coords);
        let mat = &materials[t];
        let c = stiffness(mat);
        let tbar = tet.iter().map(|&n| temperature[n as usize]).sum::<f64>() / 4.0;
        let e_th = mat.alpha * (tbar - t_ref);
        let sig_th = mat_vec6(&c, &[e_th, e_th, e_th, 0.0, 0.0, 0.0]);

        // CB = C·B (6×12).
        let mut cb = [[0.0; 12]; 6];
        for k in 0..6 {
            for j in 0..12 {
                for l in 0..6 {
                    cb[k][j] += c[k][l] * b[l][j];
                }
            }
        }
        let gdof = |i: usize| 3 * tet[i / 3] as usize + i % 3;
        for i in 0..12 {
            let gi = gdof(i);
            let Some(fi) = free_of[gi] else { continue };
            // Thermal load fe_i = V Σ_k B[k][i] (Cε_th)[k].
            let mut fe = 0.0;
            for k in 0..6 {
                fe += b[k][i] * sig_th[k];
            }
            rhs[fi] += vol * fe;
            for j in 0..12 {
                let mut ke = 0.0;
                for k in 0..6 {
                    ke += b[k][i] * cb[k][j];
                }
                let ke = vol * ke;
                let gj = gdof(j);
                match (free_of[gj], prescribed[gj]) {
                    (Some(fj), _) => trips.push(Triplet::new(fi, fj, ke)),
                    (None, Some(u)) => rhs[fi] -= ke * u,
                    (None, None) => unreachable!(),
                }
            }
        }
    }

    let mut u = vec![0.0; n_dof];
    for d in 0..n_dof {
        if let Some(v) = prescribed[d] {
            u[d] = v;
        }
    }
    if n_free > 0 {
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &trips)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        let mut x: Mat<f64> = Mat::from_fn(n_free, 1, |i, _| rhs[i]);
        lu.solve_in_place(x.as_mut());
        for d in 0..n_dof {
            if let Some(f) = free_of[d] {
                u[d] = x[(f, 0)];
            }
        }
        if u.iter().any(|v| !v.is_finite()) {
            return Err(ElectrostaticError::Factorization(
                "singular elasticity system: rigid-body modes are not fully constrained".into(),
            ));
        }
    }

    // Stress recovery σ = C (B u − ε_th).
    let mut stress = Vec::with_capacity(n_tets);
    let mut hydro = Vec::with_capacity(n_tets);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let b = strain_b(&coords);
        let mat = &materials[t];
        let tbar = tet.iter().map(|&n| temperature[n as usize]).sum::<f64>() / 4.0;
        let e_th = mat.alpha * (tbar - t_ref);
        let mut eps = [0.0; 6];
        for k in 0..6 {
            for j in 0..12 {
                eps[k] += b[k][j] * u[3 * tet[j / 3] as usize + j % 3];
            }
        }
        for k in 0..3 {
            eps[k] -= e_th;
        }
        let s = mat_vec6(&stiffness(mat), &eps);
        hydro.push((s[0] + s[1] + s[2]) / 3.0);
        stress.push(s);
    }
    let displacement = (0..n_nodes)
        .map(|n| [u[3 * n], u[3 * n + 1], u[3 * n + 2]])
        .collect();
    Ok(ThermoelasticSolution {
        displacement,
        stress,
        hydrostatic_stress: hydro,
    })
}
