//! Moved out of `problem/mod.rs` (issue #1398): pure move, no logic change.

use super::*;

/// Capacitance-spec rules (scalar, before the mesh is read). The
/// electrostatic solve is static, real and lossless: anything that only
/// has meaning for a frequency-domain solve is rejected rather than
/// silently ignored.
pub(super) fn validate_capacitance(spec: &ProblemSpec) -> Result<(), CliError> {
    let c = spec
        .capacitance
        .as_ref()
        .expect("capacitance spec has a capacitance section");
    if c.terminals.is_empty() {
        return Err(invalid(
            "capacitance.terminals must name at least one conductor surface (a dimension-2 \
             physical group)",
        ));
    }
    if c.ground.is_empty() {
        return Err(invalid(
            "capacitance.ground must name at least one grounded (0 V) reference surface — the \
             return conductor / ground plane / enclosure (required in schema v1)",
        ));
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "a capacitance spec cannot have {what}: {why} (`geode capacitance` is a static \
             electrostatic solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are frequency-domain excitations; name the conductors in \
             `capacitance.terminals` instead",
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
    if !bcs.leontovich.is_empty() {
        return Err(static_only(
            "Leontovich walls",
            "a surface impedance has no static meaning",
        ));
    }
    if !bcs.silver_muller.is_empty() {
        return Err(static_only(
            "Silver-Müller walls",
            "a radiation condition has no static meaning",
        ));
    }
    if !bcs.pec.is_empty() {
        return Err(invalid(format!(
            "a capacitance spec cannot have `boundary_conditions.pec` ({}): in electrostatics a \
             conductor must be at a known potential — list a grounded shield / enclosure under \
             `capacitance.ground`, or a conductor of interest under `capacitance.terminals` \
             (floating, charge-neutral conductors are not supported)",
            bcs.pec.join(", ")
        )));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "a capacitance spec supports only `solver.mode = \"direct\"` (sparse LU on the \
             real SPD electrostatic system)",
        ));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[1] != 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r has Im != 0; electrostatics has no loss / frequency, so a \
             capacitance spec needs a real permittivity (eps_r = [re, 0])",
            m.physical_group
        )));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[0] <= 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r must have Re > 0 for the electrostatic solve",
            m.physical_group
        )));
    }
    // Diagonal ε (issue #760): ∇·(ε∇φ) with a real SPD tensor.
    if let Some(m) = spec.materials.iter().find(|m| {
        m.eps_r_diag.is_some_and(|d| {
            d.components()
                .iter()
                .any(|&[re, im]| im != 0.0 || re <= 0.0)
        })
    }) {
        return Err(invalid(format!(
            "materials[{}].eps_r_diag must be real with Re > 0 on every axis for the \
             electrostatic solve (each component [re, 0], re > 0)",
            m.physical_group
        )));
    }
    Ok(())
}

/// Bind the capacitance terminals / ground to their tagged triangles and
/// node sets. Conductors that share a mesh node are electrically shorted
/// and rejected (the solve would silently give the shared nodes one of
/// the two potentials).
pub(super) fn resolve_capacitance(
    c: &crate::spec::CapacitanceSpec,
    terminal_tags: &[i32],
    ground_tags: &[i32],
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<CapacitanceTarget, CliError> {
    let conductor = |name: &str, tag: i32, role: &str| -> Result<Conductor, CliError> {
        let surface = surface(name, tag, role)?;
        let mut nodes: Vec<u32> = surface.triangles.iter().flatten().copied().collect();
        nodes.sort_unstable();
        nodes.dedup();
        Ok(Conductor { surface, nodes })
    };
    let terminals = c
        .terminals
        .iter()
        .zip(terminal_tags)
        .map(|(n, &t)| conductor(n, t, "capacitance terminal"))
        .collect::<Result<Vec<_>, _>>()?;
    let ground = c
        .ground
        .iter()
        .zip(ground_tags)
        .map(|(n, &t)| conductor(n, t, "capacitance ground"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ground_nodes: Vec<u32> = ground
        .iter()
        .flat_map(|g| g.nodes.iter().copied())
        .collect();
    ground_nodes.sort_unstable();
    ground_nodes.dedup();

    let shared = |a: &[u32], b: &[u32]| a.iter().filter(|n| b.binary_search(n).is_ok()).count();
    let shorted = |what: String, n: usize| {
        invalid(format!(
            "{what} share {n} mesh node(s): touching conductors are electrically shorted — \
             merge them into one terminal or separate them in the mesh"
        ))
    };
    for (i, t) in terminals.iter().enumerate() {
        for u in &terminals[i + 1..] {
            let n = shared(&t.nodes, &u.nodes);
            if n > 0 {
                return Err(shorted(
                    format!(
                        "capacitance terminals `{}` and `{}`",
                        t.surface.name, u.surface.name
                    ),
                    n,
                ));
            }
        }
        let n = shared(&t.nodes, &ground_nodes);
        if n > 0 {
            return Err(shorted(
                format!(
                    "capacitance terminal `{}` and the ground surface(s)",
                    t.surface.name
                ),
                n,
            ));
        }
    }
    Ok(CapacitanceTarget {
        terminals,
        ground,
        ground_nodes,
    })
}

/// Inductance-spec rules (scalar, before the mesh is read). The
/// magnetostatic solve is static, real and lossless: anything that only
/// has meaning for a frequency-domain solve is rejected rather than
/// silently ignored. Unlike a capacitance spec, `boundary_conditions.pec`
/// is **required**: it is the magnetic wall (`n×A = 0`) that truncates
/// the domain and the return conductor the open current paths close
/// through.
pub(super) fn validate_inductance(spec: &ProblemSpec) -> Result<(), CliError> {
    let ind = spec
        .inductance
        .as_ref()
        .expect("inductance spec has an inductance section");
    if ind.paths.is_empty() {
        return Err(invalid(
            "inductance.paths must list at least one current path (`name`, `conductor`, \
             `source`, `sink`)",
        ));
    }
    let mut names = std::collections::HashSet::new();
    let mut conductors: BTreeMap<&str, &str> = BTreeMap::new();
    for p in &ind.paths {
        if p.name.is_empty() {
            return Err(invalid("inductance.paths[].name must be non-empty"));
        }
        if !names.insert(p.name.as_str()) {
            return Err(invalid(format!(
                "inductance path name `{}` is used more than once",
                p.name
            )));
        }
        if p.source == p.sink {
            return Err(invalid(format!(
                "inductance path `{}`: source and sink are the same group `{}` — an open \
                 current path enters and leaves through two distinct faces (closed loops with \
                 no terminals are not supported in schema v1)",
                p.name, p.source
            )));
        }
        if let Some(prev) = conductors.insert(p.conductor.as_str(), p.name.as_str()) {
            return Err(invalid(format!(
                "inductance paths `{prev}` and `{}` share the conductor volume `{}`: each path \
                 needs its own conductor group (one conduction solve per volume)",
                p.name, p.conductor
            )));
        }
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "an inductance spec cannot have {what}: {why} (`geode inductance` is a static \
             magnetostatic solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are frequency-domain excitations; describe each current path in \
             `inductance.paths` instead",
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
    if !bcs.leontovich.is_empty() {
        return Err(static_only(
            "Leontovich walls",
            "a surface impedance has no static meaning",
        ));
    }
    if !bcs.silver_muller.is_empty() {
        return Err(static_only(
            "Silver-Müller walls",
            "a radiation condition has no static meaning",
        ));
    }
    if bcs.pec.is_empty() {
        return Err(invalid(
            "an inductance spec needs `boundary_conditions.pec`: the PEC wall (n x A = 0) \
             truncates the domain and is the return conductor; every current path's source and \
             sink faces must touch the same connected PEC component (e.g. the shield + end caps \
             of a coax)",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "an inductance spec supports only `solver.mode = \"direct\"` (sparse LU on the \
             tree-cotree-gauged real magnetostatic system)",
        ));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r != [1.0, 0.0]) {
        return Err(invalid(format!(
            "materials[{}].eps_r = {:?} has no effect on a magnetostatic solve; an inductance \
             spec takes only `mu_r` (omit `eps_r` or set it to [1, 0])",
            m.physical_group, m.eps_r
        )));
    }
    Ok(())
}

/// Per-tet real relative permeability from the per-tag `μ_r` table
/// (vacuum `1` for untagged / unlisted regions) — the `μ_r` twin of the
/// per-tet `ε_r` mapping above.
pub(super) fn build_mu_r(tet_tags: &[i32], mu_by_tag: &BTreeMap<i32, f64>) -> Vec<f64> {
    tet_tags
        .iter()
        .map(|t| mu_by_tag.get(t).copied().unwrap_or(1.0))
        .collect()
}

/// Bind every inductance current path: its conductor tets, source / sink
/// faces and node sets, then check the geometry the open-path conduction
/// solve needs — terminal faces on the conductor, node-disjoint source /
/// sink, each terminal touching the PEC wall (the return conductor), and
/// source and sink on the same connected PEC component
/// ([`check_path_components`]).
pub(super) fn resolve_inductance(
    ind: &crate::spec::InductanceSpec,
    path_tags: &[[i32; 3]],
    tagged: &TaggedTetMesh,
    pec: &[Surface],
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<InductanceTarget, CliError> {
    let node_set = |tris: &[[u32; 3]]| -> Vec<u32> {
        let mut n: Vec<u32> = tris.iter().flatten().copied().collect();
        n.sort_unstable();
        n.dedup();
        n
    };
    let pec_nodes = node_set(
        &pec.iter()
            .flat_map(|s| s.triangles.iter().copied())
            .collect::<Vec<_>>(),
    );
    let mut paths = Vec::with_capacity(ind.paths.len());
    for (p, &[ctag, stag, ktag]) in ind.paths.iter().zip(path_tags) {
        let conductor: Vec<bool> = tagged
            .tet_physical_tags
            .iter()
            .map(|&t| t == ctag)
            .collect();
        let n_conductor_tets = conductor.iter().filter(|&&c| c).count();
        if n_conductor_tets == 0 {
            return Err(invalid(format!(
                "inductance path `{}`: conductor volume `{}` has no tagged tets in the mesh",
                p.name, p.conductor
            )));
        }
        let source = surface(&p.source, stag, "inductance source")?;
        let sink = surface(&p.sink, ktag, "inductance sink")?;
        let face_to_tet = face_to_tet_map(&tagged.mesh, Some(&conductor));
        for (role, surf) in [("source", &source), ("sink", &sink)] {
            let off = surf
                .triangles
                .iter()
                .filter(|tri| {
                    let mut k = **tri;
                    k.sort_unstable();
                    !face_to_tet.contains_key(&k)
                })
                .count();
            if off > 0 {
                return Err(invalid(format!(
                    "inductance path `{}`: {off} of {} triangles of {role} `{}` are not faces \
                     of the conductor volume `{}` — the terminal must lie on the conductor's \
                     boundary",
                    p.name,
                    surf.triangles.len(),
                    surf.name,
                    p.conductor
                )));
            }
        }
        let source_nodes = node_set(&source.triangles);
        let sink_nodes = node_set(&sink.triangles);
        let shared = sink_nodes
            .iter()
            .filter(|n| source_nodes.binary_search(n).is_ok())
            .count();
        if shared > 0 {
            return Err(invalid(format!(
                "inductance path `{}`: source `{}` and sink `{}` share {shared} mesh node(s) — \
                 the terminals of an open path must be separated by conductor",
                p.name, p.source, p.sink
            )));
        }
        for (role, surf, nodes) in [
            ("source", &source, &source_nodes),
            ("sink", &sink, &sink_nodes),
        ] {
            if !nodes.iter().any(|n| pec_nodes.binary_search(n).is_ok()) {
                return Err(invalid(format!(
                    "inductance path `{}`: {role} `{}` does not touch any \
                     `boundary_conditions.pec` surface — the current must enter and leave \
                     through the PEC return conductor (a terminal floating in the dielectric \
                     has no return path; lumped gap sources are not supported in schema v1)",
                    p.name, surf.name
                )));
            }
        }
        paths.push(CurrentPath {
            name: p.name.clone(),
            conductor_group: p.conductor.clone(),
            conductor_tag: ctag,
            conductor,
            n_conductor_tets,
            source,
            sink,
            source_nodes,
            sink_nodes,
        });
    }
    check_path_components(&paths, tagged.mesh.n_nodes(), pec)?;
    Ok(InductanceTarget { paths })
}
