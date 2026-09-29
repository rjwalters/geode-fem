//! `geode capacitance`: static Maxwell capacitance matrix between named
//! conductor surfaces (issue #705, Epic #702 Phase 3).
//!
//! Wraps the validated library extractor
//! ([`geode_core::assembly::electrostatic`], `benchmarks/electrostatic`):
//! each `capacitance.terminals` / `capacitance.ground` physical group's
//! tagged triangles become a Dirichlet node set
//! ([`crate::problem::Conductor`]), the per-tet real `ε_r` comes from the
//! spec's `materials` (vacuum by default), and
//! [`assemble_electrostatic`] + [`extract_capacitance`] run one
//! unit-voltage solve per terminal — terminal *i* at 1 V, **every other
//! terminal and all ground at 0 V** — forming `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` with
//! the full stiffness. There is no floating (charge-neutral) conductor
//! model.
//!
//! Units: the library integrates in mesh units with `ε₀` in F/m, so its
//! capacitances are in `F/m × mesh unit`; multiplying by
//! `mesh.length_unit_m` gives farads.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use geode_core::assembly::electrostatic::{
    ConductorSurface, Electrode, assemble_electrostatic, extract_capacitance,
};
use geode_core::mesh::TetMesh;

use crate::check::{capacitance_settings_summary, mesh_summary, region_summaries};
use crate::error::CliError;
use crate::problem::{self, CapacitanceTarget};
use crate::report::{CapacitanceReport, CapacitanceSolverStats, Provenance};
use crate::spec::Analysis;

/// Relative slack of the Maxwell sign-structure check.
const SIGN_TOL: f64 = 1e-9;

/// Load, solve and report; with `spice`, also write the mutual-form
/// SPICE subcircuit there ([`crate::spice`]) and reference it in the
/// report.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    spice: Option<&Path>,
) -> Result<CapacitanceReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Capacitance))?;
    let target = p
        .capacitance
        .as_ref()
        .expect("capacitance spec has a resolved capacitance section");
    // `problem::load` rejected Im(ε_r) ≠ 0 and Re(ε_r) ≤ 0.
    let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();

    let t0 = Instant::now();
    let solved = solve(&p.tagged.mesh, &eps_r, target)?;
    let wall_time_s = t0.elapsed().as_secs_f64();

    let scale = p.length_unit_m();
    let n = solved.c.len();
    for (i, row) in solved.c.iter().enumerate() {
        if let Some(bad) = row.iter().find(|v| !v.is_finite()) {
            return Err(CliError::NonFinite {
                index: i,
                what: format!("capacitance row {i} has a non-finite entry {bad}"),
            });
        }
    }
    let c_farad: Vec<Vec<f64>> = solved
        .c
        .iter()
        .map(|row| row.iter().map(|v| v * scale).collect())
        .collect();
    let c_sigma_farad = (0..n).map(|i| solved.c_sigma(i) * scale).collect();
    let c_flux_diag_farad = solved
        .c_flux_diag
        .iter()
        .map(|q| q.map(|q| q * scale))
        .collect();

    let mut report = CapacitanceReport {
        provenance,
        kind: "capacitance",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        capacitance: capacitance_settings_summary(&p).expect("capacitance spec"),
        terminals: solved.names.clone(),
        c_farad,
        c_sigma_farad,
        c_flux_diag_farad,
        max_rel_asymmetry: solved.max_rel_asymmetry(),
        maxwell_sign_structure: solved.has_maxwell_sign_structure(SIGN_TOL),
        solver: CapacitanceSolverStats {
            method: "energy",
            inner: "direct_lu",
            wall_time_s,
        },
        spice_file: None,
    };
    if let Some(path) = spice {
        report.spice_file = Some(crate::spice::write(path, &report)?);
    }
    Ok(report)
}

/// The library capacitance matrix (mesh-unit scaled, i.e. `F/m × mesh
/// unit`) for a resolved capacitance target. The surface-flux diagonal is
/// computed only for terminals lying entirely on the mesh boundary
/// (`None` otherwise).
pub fn solve(
    mesh: &TetMesh,
    eps_r: &[f64],
    target: &CapacitanceTarget,
) -> Result<geode_core::assembly::electrostatic::CapacitanceMatrix, CliError> {
    let electrodes: Vec<Electrode> = target
        .terminals
        .iter()
        .map(|t| Electrode {
            name: t.surface.name.clone(),
            nodes: t.nodes.clone(),
            voltage: 0.0,
        })
        .collect();
    let rho = vec![0.0_f64; mesh.n_tets()];
    let sys = assemble_electrostatic(mesh, eps_r, &rho, &electrodes, &target.ground_nodes)?;

    // Flux cross-check only where it is well defined: a one-sided surface
    // (every triangle a boundary face owned by exactly one tet).
    let boundary = boundary_faces(mesh);
    let on_boundary: Vec<bool> = target
        .terminals
        .iter()
        .map(|t| {
            t.surface
                .triangles
                .iter()
                .all(|tri| boundary.contains(&sorted3(*tri)))
        })
        .collect();
    let surfaces: Vec<ConductorSurface> = target
        .terminals
        .iter()
        .zip(&on_boundary)
        .map(|(t, &ok)| ConductorSurface {
            triangles: if ok {
                t.surface.triangles.clone()
            } else {
                Vec::new()
            },
        })
        .collect();
    let mut c = extract_capacitance(
        &sys,
        mesh,
        eps_r,
        &electrodes,
        &target.ground_nodes,
        &surfaces,
    )?;
    for (q, ok) in c.c_flux_diag.iter_mut().zip(&on_boundary) {
        if !ok {
            *q = None;
        }
    }
    Ok(c)
}

/// Faces (sorted node triples) owned by exactly one tet.
fn boundary_faces(mesh: &TetMesh) -> HashSet<[u32; 3]> {
    const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let mut count: HashMap<[u32; 3], u8> = HashMap::with_capacity(mesh.n_tets() * 2);
    for tet in &mesh.tets {
        for f in FACES {
            *count
                .entry(sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]))
                .or_default() += 1;
        }
    }
    count
        .into_iter()
        .filter_map(|(k, n)| (n == 1).then_some(k))
        .collect()
}

fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_faces_of_one_tet_are_all_four() {
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            tets: vec![[0, 1, 2, 3], [1, 2, 3, 4]],
            ..Default::default()
        };
        let b = boundary_faces(&mesh);
        // 8 faces, the shared [1,2,3] is interior.
        assert_eq!(b.len(), 6);
        assert!(!b.contains(&[1, 2, 3]));
        assert!(b.contains(&[0, 1, 2]));
    }
}
