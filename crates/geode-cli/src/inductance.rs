//! `geode inductance`: static Maxwell inductance matrix between named open
//! current paths (issue #714, Epic #702 Phase 3b).
//!
//! Per path ([`crate::problem::CurrentPath`]): the library's open-path
//! construction ([`open_path_current`]) solves a P1 conduction problem on
//! the path's conductor volume (`φ = 1` on the source face, `0` on the
//! sink, insulated elsewhere) and returns `J = −σ∇φ` normalised to a 1 A
//! Galerkin net current. [`assemble_magnetostatic3d`] then assembles the
//! `ν₀/μ_r`-weighted Nédélec curl-curl with the spec's PEC wall plus the
//! terminal contacts eliminated, and [`extract_inductance`] runs one
//! tree-cotree-gauged solve per path — each after the discrete-
//! solenoidality gate — forming `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)`.
//!
//! Units: the library integrates in mesh units with `μ₀` in H/m, so its
//! inductances are in `H/m × mesh unit`; multiplying by
//! `mesh.length_unit_m` gives henries.

use std::path::Path;
use std::time::Instant;

use geode_core::assembly::current_path::{
    CurrentPathError, OpenPathCurrent, check_grounded_balance, open_path_current, ungrounded_nodes,
};
use geode_core::assembly::magnetostatic3d::{
    InductanceMatrix, assemble_current_rhs, assemble_magnetostatic3d, check_solenoidal,
    extract_inductance,
};
use geode_core::mesh::TetMesh;

use crate::check::{inductance_settings_summary, mesh_summary, region_summaries};
use crate::error::CliError;
use crate::problem::{self, InductanceTarget};
use crate::report::{InductanceReport, InductanceSolverStats, Provenance};
use crate::spec::Analysis;

/// Relative tolerance of the discrete-solenoidality gate (the conduction
/// construction passes at round-off, ~1e-14).
pub const SOLENOIDAL_TOL: f64 = 1e-6;

/// Cholesky slack of the SPD check (the matrix is in `H/m × mesh unit`,
/// i.e. `~μ₀` scale, so a zero tolerance is the honest threshold).
const SPD_TOL: f64 = 0.0;

/// Load, solve and report; with `spice`, also write the SPICE
/// subcircuit ([`crate::spice::write_inductance`]) there.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    spice: Option<&Path>,
) -> Result<InductanceReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Inductance))?;
    let target = p
        .inductance
        .as_ref()
        .expect("inductance spec has a resolved inductance section");

    let t0 = Instant::now();
    let solved = solve(&p.tagged.mesh, &p.mu_r, &p.pec_mask, target)?;
    let wall_time_s = t0.elapsed().as_secs_f64();
    check_physical(&solved.matrix)?;

    let scale = p.length_unit_m();
    for (i, row) in solved.matrix.l.iter().enumerate() {
        if let Some(bad) = row.iter().find(|v| !v.is_finite()) {
            return Err(CliError::NonFinite {
                index: i,
                what: format!("inductance row {i} has a non-finite entry {bad}"),
            });
        }
    }
    let l_henry = solved
        .matrix
        .l
        .iter()
        .map(|row| row.iter().map(|v| v * scale).collect())
        .collect();
    let paths = &solved.paths;
    let mut report = InductanceReport {
        provenance,
        kind: "inductance",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        inductance: inductance_settings_summary(&p).expect("inductance spec"),
        paths: solved.matrix.names.clone(),
        l_henry,
        flux_linkage_diag_henry: solved
            .matrix
            .flux_linkage_diag
            .iter()
            .map(|v| v * scale)
            .collect(),
        current_a: paths.iter().map(|c| c.terminal.current).collect(),
        sink_current_a: paths.iter().map(|c| c.sink_current).collect(),
        source_face_flux_a: paths.iter().map(|c| c.source_face_flux).collect(),
        sink_face_flux_a: paths.iter().map(|c| c.sink_face_flux).collect(),
        max_solenoidal_residual: solved.max_solenoidal_residual,
        max_rel_asymmetry: solved.matrix.max_rel_asymmetry(),
        // Guaranteed by `check_physical` above (kept in the report schema).
        is_spd: solved.matrix.is_spd(SPD_TOL),
        solver: InductanceSolverStats {
            method: "energy",
            inner: "direct_lu",
            solenoidal_tol: SOLENOIDAL_TOL,
            wall_time_s,
        },
        spice_file: None,
    };
    if let Some(path) = spice {
        report.spice_file = Some(crate::spice::write_inductance(path, &report)?);
    }
    Ok(report)
}

/// A solved inductance target: the library matrix (in `H/m × mesh unit`)
/// plus each path's excitation diagnostics.
#[derive(Debug, Clone)]
pub struct Solved {
    /// The library inductance matrix (mesh-unit scaled).
    pub matrix: InductanceMatrix,
    /// Per path, the open-path excitation (in matrix order).
    pub paths: Vec<OpenPathCurrent>,
    /// Largest discrete-divergence residual over the paths.
    pub max_solenoidal_residual: f64,
}

/// Build every path's excitation and extract the inductance matrix
/// (`H/m × mesh unit`). `pec_mask` must already eliminate the PEC wall and
/// the terminal contacts ([`crate::problem::Problem::pec_mask`]).
pub fn solve(
    mesh: &TetMesh,
    mu_r: &[f64],
    pec_mask: &[bool],
    target: &InductanceTarget,
) -> Result<Solved, CliError> {
    // Homogeneous conductors: the current distribution does not depend on
    // the conductivity value (see the `current_path` module docs).
    let sigma_r = vec![1.0_f64; mesh.n_tets()];
    let paths = target
        .paths
        .iter()
        .map(|c| {
            open_path_current(
                mesh,
                &c.name,
                &c.conductor,
                &sigma_r,
                &c.source.triangles,
                &c.sink.triangles,
            )
            .map_err(|e| path_error(&c.name, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let sys = assemble_magnetostatic3d(mesh, mu_r, pec_mask)?;
    let mut max_res = 0.0_f64;
    for path in &paths {
        // By construction (terminal faces are PEC contacts) every terminal
        // node is grounded; anything else would make the curl-curl source
        // problem inconsistent.
        let floating = ungrounded_nodes(&sys, &path.terminal.exempt_nodes);
        assert!(
            floating.is_empty(),
            "path {}: {} terminal nodes not on the PEC wall despite the contact mask",
            path.terminal.name,
            floating.len()
        );
        // Per-component balance (defense in depth: `problem::load` already
        // rejects a path whose source and sink touch different PEC
        // components, and this recomputes it on the assembled system).
        check_grounded_balance(&sys, mesh, &path.terminal, SOLENOIDAL_TOL)
            .map_err(|e| path_error(&path.terminal.name, e))?;
        let b = assemble_current_rhs(&sys, mesh, &path.terminal.j)?;
        max_res = max_res.max(check_solenoidal(
            &sys,
            &b,
            &path.terminal.exempt_nodes,
            SOLENOIDAL_TOL,
        )?);
    }
    let terminals: Vec<_> = paths.iter().map(|c| c.terminal.clone()).collect();
    let matrix = extract_inductance(&sys, mesh, &terminals, SOLENOIDAL_TOL)?;
    Ok(Solved {
        matrix,
        paths,
        max_solenoidal_residual: max_res,
    })
}

/// Post-solve physical gate: a static inductance matrix is symmetric
/// positive definite (the magnetic energy `½ Iᵀ L I` is positive for any
/// non-zero current vector). A non-positive self inductance or a failed
/// Cholesky means the source problem was inconsistent (e.g. an unbalanced
/// PEC return path that slipped past the pre-solve checks) — report it as a
/// `solve_failed` error rather than a status-`ok` answer.
pub fn check_physical(m: &InductanceMatrix) -> Result<(), CliError> {
    for (i, name) in m.names.iter().enumerate() {
        let lii = m.l[i][i];
        if lii.is_nan() || lii <= 0.0 {
            return Err(CliError::NonPhysicalInductance(format!(
                "self inductance L[{name}][{name}] = {lii:e} is not positive"
            )));
        }
    }
    if !m.is_spd(SPD_TOL) {
        return Err(CliError::NonPhysicalInductance(format!(
            "the inductance matrix over paths [{}] is not symmetric positive definite",
            m.names.join(", ")
        )));
    }
    Ok(())
}

/// Geometry problems of a current path are spec errors (`invalid_spec`);
/// only a failed conduction factorization is a solve failure.
fn path_error(name: &str, e: CurrentPathError) -> CliError {
    match e {
        CurrentPathError::Conduction(_) | CurrentPathError::ShapeMismatch(_) => {
            CliError::CurrentPath(e)
        }
        other => CliError::InvalidSpec(format!("inductance path `{name}`: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(l: Vec<Vec<f64>>) -> InductanceMatrix {
        let n = l.len();
        InductanceMatrix {
            names: (0..n).map(|i| format!("p{i}")).collect(),
            flux_linkage_diag: (0..n).map(|i| l[i][i]).collect(),
            l,
        }
    }

    #[test]
    fn physical_gate_accepts_spd() {
        check_physical(&matrix(vec![vec![2.7e-10]])).unwrap();
        check_physical(&matrix(vec![vec![2.0e-9, 1.0e-9], vec![1.0e-9, 3.0e-9]])).unwrap();
    }

    #[test]
    fn physical_gate_rejects_non_positive_diagonal() {
        // The PR #718 repro value (shield-less coax).
        for bad in [-327.68, 0.0, f64::NAN] {
            let e = check_physical(&matrix(vec![vec![bad]])).unwrap_err();
            assert_eq!(e.code(), "solve_failed", "{e}");
            assert!(e.to_string().contains("not positive"), "{e}");
        }
    }

    #[test]
    fn physical_gate_rejects_indefinite_with_positive_diagonal() {
        // |k| > 1: positive diagonal, indefinite matrix.
        let e =
            check_physical(&matrix(vec![vec![1.0e-9, 2.0e-9], vec![2.0e-9, 1.0e-9]])).unwrap_err();
        assert_eq!(e.code(), "solve_failed");
        assert!(e.to_string().contains("positive definite"), "{e}");
    }
}
