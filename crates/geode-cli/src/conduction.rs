//! `geode conduction`: DC terminal conductance matrix between named
//! terminal surfaces (issue #1479).
//!
//! Wraps [`geode_core::assembly::conduction`]: each `conduction.terminals`
//! physical group's tagged triangles become a Dirichlet node set (or, with
//! `conduction.r_ext`, a node set held through a lumped resistance), the
//! per-tet conductivity comes from `materials[].conductivity_s_m`
//! (unlisted regions are insulating), and one unit-voltage solve per
//! terminal gives `G` (`I_i = Σ_j G_ij V_j`).
//!
//! **Operating point.** The reported terminal currents, total power and
//! power balance are for the **first terminal at 1 V and every other
//! terminal at 0 V**.
//!
//! Units: the library integrates in mesh units with `σ` in S/m, so `G`,
//! currents and power carry one factor of the mesh length unit `L`
//! (multiplied back in here); an `r_ext` of `R` Ω enters the library as
//! `R·L`. `J` scales by `1/L` and the Joule density by `1/L²`.

use std::path::Path;
use std::time::Instant;

use geode_core::assembly::conduction::{
    ConductionOptions, ConductionTerminal, assemble_conduction,
};

use crate::check::{mesh_summary, region_summaries};
use crate::error::CliError;
use crate::export::OutDir;
use crate::problem;
use crate::report::{
    ConductionReport, ConductionSettingsSummary, ConductionSolverStats, ConductorSummary,
    Provenance,
};
use crate::spec::Analysis;

/// Name of the per-element field file written under `--outdir`.
pub const FIELDS_FILE_NAME: &str = "conduction_fields.vtu";

/// Load, solve and report.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
) -> Result<ConductionReport, CliError> {
    // Create `--outdir` up front so an unwritable path fails before the solve.
    let out = OutDir::create_opt(outdir)?;
    let p = problem::load(spec_path, Some(Analysis::Conduction))?;
    let target = p
        .conduction
        .as_ref()
        .expect("conduction spec has a resolved conduction section");
    let mesh = &p.tagged.mesh;
    let scale = p.length_unit_m();

    let terminals: Vec<ConductionTerminal> = target
        .terminals
        .iter()
        .zip(&target.r_ext)
        .map(|(t, r)| ConductionTerminal {
            name: t.surface.name.clone(),
            nodes: t.nodes.clone(),
            r_ext: r.map(|r| r * scale),
        })
        .collect();

    let t0 = Instant::now();
    let sys = assemble_conduction(
        mesh,
        &target.sigma_s_m,
        &terminals,
        ConductionOptions::default(),
    )?;
    let gm = sys.conductance_matrix(mesh)?;
    let mut v = vec![0.0; terminals.len()];
    v[0] = 1.0;
    let sol = sys.solve(mesh, &v)?;
    let wall_time_s = t0.elapsed().as_secs_f64();

    let g_siemens: Vec<Vec<f64>> =
        gm.g.iter()
            .map(|row| row.iter().map(|x| x * scale).collect())
            .collect();
    for (i, row) in g_siemens.iter().enumerate() {
        if let Some(bad) = row.iter().find(|x| !x.is_finite()) {
            return Err(CliError::NonFinite {
                index: i,
                what: format!("conductance row {i} has a non-finite entry {bad}"),
            });
        }
    }
    let terminal_current_a: Vec<f64> = sol.terminal_current.iter().map(|i| i * scale).collect();
    let n_nodes = mesh.n_nodes();
    let n_pinned: usize = target
        .terminals
        .iter()
        .zip(&target.r_ext)
        .filter(|(_, r)| r.is_none())
        .map(|(t, _)| t.nodes.len())
        .sum();
    let settings = ConductionSettingsSummary {
        terminals: target
            .terminals
            .iter()
            .map(|k| ConductorSummary {
                physical_group: k.surface.name.clone(),
                tag: k.surface.tag,
                n_triangles: k.surface.triangles.len(),
                n_nodes: k.nodes.len(),
            })
            .collect(),
        r_ext_ohm: target.r_ext.clone(),
        operating_point_v: v.clone(),
        element: "p1_tet",
        n_dof: n_nodes,
        n_free_dof: n_nodes - n_pinned,
        n_solves: terminals.len(),
    };
    let max_j_a_m2 = sol
        .j
        .iter()
        .map(|j| (j[0] * j[0] + j[1] * j[1] + j[2] * j[2]).sqrt())
        .fold(0.0, f64::max)
        / scale;
    let max_joule_w_m3 = sol.joule_density.iter().copied().fold(0.0, f64::max) / (scale * scale);

    let fields_file = out
        .as_ref()
        .map(|o| {
            let j_si: Vec<[f64; 3]> = sol
                .j
                .iter()
                .map(|j| [j[0] / scale, j[1] / scale, j[2] / scale])
                .collect();
            let joule_si: Vec<f64> = sol
                .joule_density
                .iter()
                .map(|q| q / (scale * scale))
                .collect();
            o.write_cell_fields(FIELDS_FILE_NAME, mesh, &j_si, &joule_si, &target.sigma_s_m)
        })
        .transpose()?;

    Ok(ConductionReport {
        provenance,
        kind: "conduction",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        conduction: settings,
        terminals: gm.names.clone(),
        g_siemens,
        max_rel_asymmetry: gm.max_rel_asymmetry(),
        max_rel_row_sum: gm.max_rel_row_sum(),
        terminal_current_a,
        total_power_w: sol.power_volume * scale + sol.power_resistors * scale,
        power_balance_rel_error: sol.power_rel_mismatch,
        max_current_density_a_m2: max_j_a_m2,
        max_joule_density_w_m3: max_joule_w_m3,
        per_element_fields: if fields_file.is_some() {
            "exported: see `fields_file`"
        } else {
            "not_exported: pass `--outdir DIR` to write the per-element fields"
        },
        fields_file,
        solver: ConductionSolverStats {
            method: "gradient_energy",
            inner: "direct_lu_jacobi_scaled",
            max_rel_residual: gm.max_residual.max(sol.residual),
            wall_time_s,
        },
    })
}
