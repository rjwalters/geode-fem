//! Conduction-spec validation and resolution (issue #1479).

use super::*;

/// Conduction-spec rules (scalar, before the mesh is read). The DC
/// conduction solve is static and real: anything that only has meaning
/// for a frequency-domain, electrostatic or magnetostatic solve is
/// rejected rather than silently ignored.
pub(super) fn validate_conduction(spec: &ProblemSpec) -> Result<(), CliError> {
    let c = spec
        .conduction
        .as_ref()
        .expect("conduction spec has a conduction section");
    if c.terminals.is_empty() {
        return Err(invalid(
            "conduction.terminals must name at least one terminal surface (a dimension-2 \
             physical group)",
        ));
    }
    for (name, r) in &c.r_ext {
        if !c.terminals.contains(name) {
            return Err(invalid(format!(
                "conduction.r_ext names `{name}`, which is not in conduction.terminals"
            )));
        }
        if !(r.is_finite() && *r > 0.0) {
            return Err(invalid(format!(
                "conduction.r_ext[{name}] must be finite and > 0 (got {r})"
            )));
        }
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "a conduction spec cannot have {what}: {why} (`geode conduction` is a static DC \
             solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are frequency-domain excitations; name the contacts in \
             `conduction.terminals` instead",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(static_only(
            "`wave_ports`",
            "wave ports are frequency-domain modal boundaries",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(static_only("`frequencies`", "there is no frequency"));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(static_only(
            "`absorbing_regions`",
            "UPML is a frequency-domain absorber",
        ));
    }
    let bcs = &spec.boundary_conditions;
    if !bcs.leontovich.is_empty() || !bcs.silver_muller.is_empty() || !bcs.pec.is_empty() {
        return Err(static_only(
            "`boundary_conditions` (pec / leontovich / silver_muller)",
            "terminals are the only Dirichlet surfaces; every other boundary is insulating",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "a conduction spec supports only `solver.mode = \"direct\"` (sparse LU)",
        ));
    }
    for m in &spec.materials {
        if m.conductivity_s_m.is_none() {
            return Err(invalid(format!(
                "materials[{}] has no `conductivity_s_m`: every material of a conduction spec \
                 needs its conductivity in S/m (unlisted regions are insulating)",
                m.physical_group
            )));
        }
        if m.dispersion.is_some() || m.eps_r_diag.is_some() || m.mu_r_diag.is_some() {
            return Err(invalid(format!(
                "materials[{}]: `dispersion` / `eps_r_diag` / `mu_r_diag` have no meaning for a \
                 DC conduction solve",
                m.physical_group
            )));
        }
    }
    Ok(())
}

/// Bind the conduction terminals to their tagged triangles and node sets.
/// Terminals that share a mesh node are electrically shorted and rejected.
pub(super) fn resolve_conduction(
    c: &crate::spec::ConductionSpec,
    terminal_tags: &[i32],
    sigma_s_m: Vec<f64>,
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<ConductionTarget, CliError> {
    let mut terminals: Vec<Conductor> = Vec::with_capacity(c.terminals.len());
    for (n, &tag) in c.terminals.iter().zip(terminal_tags) {
        let surface = surface(n, tag, "conduction terminal")?;
        let mut nodes: Vec<u32> = surface.triangles.iter().flatten().copied().collect();
        nodes.sort_unstable();
        nodes.dedup();
        terminals.push(Conductor { surface, nodes });
    }
    for (i, t) in terminals.iter().enumerate() {
        for u in &terminals[i + 1..] {
            let n = t
                .nodes
                .iter()
                .filter(|x| u.nodes.binary_search(x).is_ok())
                .count();
            if n > 0 {
                return Err(invalid(format!(
                    "conduction terminals `{}` and `{}` share {n} mesh node(s): touching \
                     terminals are electrically shorted — merge them into one terminal or \
                     separate them in the mesh",
                    t.surface.name, u.surface.name
                )));
            }
        }
    }
    let r_ext = c
        .terminals
        .iter()
        .map(|n| c.r_ext.get(n).copied())
        .collect();
    Ok(ConductionTarget {
        terminals,
        r_ext,
        sigma_s_m,
    })
}
