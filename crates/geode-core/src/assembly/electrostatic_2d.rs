//! Host-side **2-D scalar electrostatic** assembly and Maxwell
//! capacitance-matrix extraction on a [`TriMesh`] (P1), the planar sibling
//! of [`crate::assembly::electrostatic`] (issue #1362).
//!
//! It solves `−∇·(ε₀ ε_r ∇φ) = ρ` on a 2-D cross-section with the same
//! contract as the 3-D path: per-triangle scalar `ε_r` and `ρ`, named
//! Dirichlet [`Electrode`] node sets, a ground node set, `Result`-returning
//! errors (no panics on bad input), per-triangle `E = −∇φ` recovery, and the
//! Maxwell capacitance matrix by the energy method `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾`.
//!
//! Units are per unit length: `ρ` is C/m² and the capacitance matrix is in
//! F/m. The returned [`ElectrostaticSystem`] and [`CapacitanceMatrix`] are
//! the 3-D types, so [`ElectrostaticSystem::solve`] and every
//! [`CapacitanceMatrix`] accessor work unchanged. The surface-flux
//! cross-check is not available in 2-D, so `c_flux_diag` is all `None`.
//! An axisymmetric (r, z) variant is out of scope.

use std::collections::BTreeSet;

use faer::sparse::Triplet;

use crate::analytic::waveguide::{TriMesh, tri_bary_grads, tri_p1_local};
use crate::assembly::electrostatic::{
    CapacitanceMatrix, EPS_0, Electrode, ElectrostaticError, ElectrostaticSystem,
    finish_electrostatic_assembly, quad_form,
};
use crate::assembly::p1::SparsityPattern;

/// Reject any node index at or beyond `n_nodes` (returns `Err`, never panics).
fn check_nodes(what: &str, nodes: &[u32], n_nodes: usize) -> Result<(), ElectrostaticError> {
    match nodes.iter().find(|&&g| (g as usize) >= n_nodes) {
        Some(g) => Err(ElectrostaticError::ShapeMismatch(format!(
            "{what} node index {g} out of range for {n_nodes} nodes"
        ))),
        None => Ok(()),
    }
}

fn check_terminals(
    electrodes: &[Electrode],
    ground: &[u32],
    n_nodes: usize,
) -> Result<(), ElectrostaticError> {
    for e in electrodes {
        if e.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(e.name.clone()));
        }
        check_nodes(&format!("electrode '{}'", e.name), &e.nodes, n_nodes)?;
    }
    check_nodes("ground", ground, n_nodes)
}

fn sparsity_from_tris(tris: &[[u32; 3]]) -> SparsityPattern {
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in tris {
        for &a in tri {
            for &b in tri {
                set.insert((a, b));
            }
        }
    }
    let (rows, cols) = set.into_iter().unzip();
    SparsityPattern { rows, cols }
}

fn tri_coords(mesh: &TriMesh, tri: &[u32; 3]) -> [[f64; 2]; 3] {
    [
        mesh.nodes[tri[0] as usize],
        mesh.nodes[tri[1] as usize],
        mesh.nodes[tri[2] as usize],
    ]
}

/// Check every triangle references existing nodes.
fn check_connectivity(mesh: &TriMesh) -> Result<(), ElectrostaticError> {
    let n = mesh.n_nodes();
    for tri in &mesh.tris {
        check_nodes("triangle", tri, n)?;
    }
    Ok(())
}

/// Assemble the reduced SPD 2-D electrostatic system for
/// `−∇·(ε₀ ε_r ∇φ) = ρ` with multi-conductor Dirichlet electrodes and a
/// ground node set pinned to `φ = 0`. The full pre-elimination stiffness is
/// retained on the returned system for the energy method.
///
/// * `eps_r`, `rho` — per-triangle, length `mesh.n_tris()` (`ρ` in C/m²).
/// * `electrodes` — named conductor node sets (`voltage` used by a bare
///   forward [`ElectrostaticSystem::solve`]).
/// * `ground` — node indices pinned to `φ = 0`.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] on a length mismatch or an
/// out-of-range node index; [`ElectrostaticError::EmptyElectrode`] for an
/// electrode with no nodes; [`ElectrostaticError::Factorization`] if no
/// Dirichlet node exists at all (singular system);
/// [`ElectrostaticError::Assembly`] if faer rejects the triplets.
pub fn assemble_electrostatic_2d(
    mesh: &TriMesh,
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<ElectrostaticSystem, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tris = mesh.n_tris();
    if eps_r.len() != n_tris {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r length {} != triangle count {n_tris}",
            eps_r.len()
        )));
    }
    if rho.len() != n_tris {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "rho length {} != triangle count {n_tris}",
            rho.len()
        )));
    }
    check_connectivity(mesh)?;
    check_terminals(electrodes, ground, n_nodes)?;
    if ground.is_empty() && electrodes.is_empty() {
        return Err(ElectrostaticError::Factorization(
            "no Dirichlet node (empty ground and no electrodes): system is singular".into(),
        ));
    }

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tris * 9);
    let mut b_full = vec![0.0_f64; n_nodes];
    for (t, tri) in mesh.tris.iter().enumerate() {
        let (k_local, m_local, _signed_area) = tri_p1_local(&tri_coords(mesh, tri));
        let eps_t = EPS_0 * eps_r[t];
        for p in 0..3 {
            let gp = tri[p] as usize;
            for q in 0..3 {
                b_full[gp] += m_local[p][q] * rho[t];
                trips.push(Triplet::new(gp, tri[q] as usize, eps_t * k_local[p][q]));
            }
        }
    }

    finish_electrostatic_assembly(
        n_nodes,
        sparsity_from_tris(&mesh.tris),
        trips,
        b_full,
        electrodes,
        ground,
    )
}

/// Per-triangle `E = −∇φ` from a full-length nodal potential (P1, so `E` is
/// constant on each triangle).
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] if `phi.len() != mesh.n_nodes()` or
/// the mesh connectivity references a missing node.
pub fn recover_e_field_2d(
    mesh: &TriMesh,
    phi: &[f64],
) -> Result<Vec<[f64; 2]>, ElectrostaticError> {
    if phi.len() != mesh.n_nodes() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "phi length {} != node count {}",
            phi.len(),
            mesh.n_nodes()
        )));
    }
    check_connectivity(mesh)?;
    Ok(mesh
        .tris
        .iter()
        .map(|tri| {
            let (grads, _gram, _signed, _abs) = tri_bary_grads(&tri_coords(mesh, tri));
            let mut g = [0.0_f64; 2];
            for p in 0..3 {
                let ap = phi[tri[p] as usize];
                g[0] += ap * grads[p][0];
                g[1] += ap * grads[p][1];
            }
            [-g[0], -g[1]]
        })
        .collect())
}

/// Maxwell capacitance matrix (F/m) by the energy method from `N`
/// unit-voltage solves on an assembled 2-D system: conductor *i* at 1 V,
/// every other conductor and the ground at 0 V, `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾`.
///
/// `conductors` and `ground` must be the sets given to
/// [`assemble_electrostatic_2d`].
///
/// # Errors
///
/// [`ElectrostaticError::EmptyElectrode`] / [`ElectrostaticError::ShapeMismatch`]
/// for an empty or out-of-range terminal;
/// [`ElectrostaticError::Factorization`] if a solve fails.
pub fn extract_capacitance_2d(
    sys: &ElectrostaticSystem,
    conductors: &[Electrode],
    ground: &[u32],
) -> Result<CapacitanceMatrix, ElectrostaticError> {
    let n = conductors.len();
    check_terminals(conductors, ground, sys.n_nodes)?;

    let mut phis: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut pinned_value = vec![0.0_f64; sys.n_nodes];
        for (k, c) in conductors.iter().enumerate() {
            let v = if k == i { 1.0 } else { 0.0 };
            for &node in &c.nodes {
                pinned_value[node as usize] = v;
            }
        }
        phis.push(sys.solve_with_pinned(&pinned_value)?);
    }

    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = quad_form(&sys.k_full, &phis[i], &phis[j]);
            c[i][j] = v;
            c[j][i] = v;
        }
    }

    Ok(CapacitanceMatrix {
        names: conductors.iter().map(|c| c.name.clone()).collect(),
        c,
        c_flux_diag: vec![None; n],
    })
}
