//! `geode check`: validate a problem spec + mesh without solving.

use std::path::Path;

use crate::error::CliError;
use crate::problem::{self, MaterialSource, Problem};
use crate::report::{
    CheckReport, EigenSettingsSummary, ExtractSettingsSummary, FrequencySummary, LeontovichSummary,
    MeshSummary, PecSummary, PortSummary, Provenance, RegionSummary, SolverSummary,
};
use crate::spec::SolverSpec;

/// Load + resolve the spec and summarize it (no assembly, no solve).
pub fn run(spec_path: &Path, provenance: Provenance) -> Result<CheckReport, CliError> {
    let p = problem::load(spec_path, None)?;
    Ok(CheckReport {
        provenance,
        kind: "check",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        pec: pec_summaries(&p),
        leontovich: p
            .leontovich
            .iter()
            .map(|l| LeontovichSummary {
                physical_group: l.surface.name.clone(),
                tag: l.surface.tag,
                n_triangles: l.surface.triangles.len(),
                conductivity_s_m: l.sigma_s_m,
                conductivity_natural: l.sigma_natural,
            })
            .collect(),
        ports: port_summaries(&p),
        frequencies: p
            .frequencies
            .iter()
            .map(|f| FrequencySummary {
                frequency_hz: f.hz,
                k0: f.k0,
            })
            .collect(),
        solver: solver_summary(p.solver()),
        analysis: p.analysis.name(),
        eigen: eigen_settings_summary(&p),
        extract: extract_settings_summary(&p),
    })
}

/// Volume regions with applied permittivities.
pub fn region_summaries(p: &Problem) -> Vec<RegionSummary> {
    p.regions
        .iter()
        .map(|r| RegionSummary {
            physical_group: r.name.clone(),
            tag: r.tag,
            n_tets: r.n_tets,
            eps_r: [r.eps_r.re, r.eps_r.im],
            eps_r_source: match r.source {
                MaterialSource::Spec => "spec",
                MaterialSource::DefaultVacuum => "default_vacuum",
            },
        })
        .collect()
}

/// PEC surfaces.
pub fn pec_summaries(p: &Problem) -> Vec<PecSummary> {
    p.pec
        .iter()
        .map(|s| PecSummary {
            physical_group: s.name.clone(),
            tag: s.tag,
            n_triangles: s.triangles.len(),
        })
        .collect()
}

/// Echo of the resolved `eigen` section (`None` for a driven spec).
pub fn eigen_settings_summary(p: &Problem) -> Option<EigenSettingsSummary> {
    p.eigen.map(|e| EigenSettingsSummary {
        n_modes: e.n_modes,
        shift_hz: e.shift.hz,
        shift_k0: e.shift.k0,
        sigma: e.sigma(),
        max_iters: e.max_iters,
        tol: e.tol,
        residual_tol: e.residual_tol,
    })
}

/// Echo of the resolved `extract` section (`None` unless an extract spec).
pub fn extract_settings_summary(p: &Problem) -> Option<ExtractSettingsSummary> {
    p.extract.as_ref().map(|x| ExtractSettingsSummary {
        anchor_source: x.source.name(),
        anchor_frequencies: x
            .anchors
            .iter()
            .map(|&i| FrequencySummary {
                frequency_hz: p.frequencies[i].hz,
                k0: p.frequencies[i].k0,
            })
            .collect(),
        l0_rel_tol: x.l0_rel_tol,
    })
}

/// Mesh summary with DOF counts.
pub fn mesh_summary(p: &Problem) -> MeshSummary {
    MeshSummary {
        path: p.mesh_path.display().to_string(),
        sha256: p.mesh_sha256.clone(),
        length_unit_m: p.length_unit_m(),
        n_nodes: p.tagged.mesh.n_nodes(),
        n_tets: p.tagged.mesh.n_tets(),
        n_edges: p.edges.len(),
        n_interior: p.n_interior(),
    }
}

/// Port summaries in matrix order.
pub fn port_summaries(p: &Problem) -> Vec<PortSummary> {
    p.ports
        .iter()
        .enumerate()
        .map(|(index, port)| PortSummary {
            index,
            physical_group: port.surface.name.clone(),
            tag: port.surface.tag,
            n_triangles: port.surface.triangles.len(),
            e_hat: port.e_hat,
            width: port.width,
            length: port.length,
            geometry_derived: port.geometry_derived,
            resistance_ohm: port.resistance_ohm,
            v_inc: [port.v_inc.re, port.v_inc.im],
        })
        .collect()
}

/// Echo of the solver selection.
pub fn solver_summary(s: SolverSpec) -> SolverSummary {
    match s {
        SolverSpec::Direct {} => SolverSummary {
            mode: "direct",
            tol: None,
            max_iters: None,
        },
        SolverSpec::Iterative { tol, max_iters } => SolverSummary {
            mode: "iterative",
            tol: Some(tol),
            max_iters: Some(max_iters),
        },
    }
}
