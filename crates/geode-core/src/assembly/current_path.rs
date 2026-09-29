//! General **open-path current excitation** for the 3-D magnetostatic
//! inductance extractor (issue #714, Epic #702 Phase 3b).
//!
//! [`crate::assembly::magnetostatic3d::extract_inductance`] is driven by
//! [`CurrentTerminal`]s — a per-tet constant current density `J` plus its net
//! current. The analytic builders in that module
//! ([`axial_current_density`](crate::assembly::magnetostatic3d::axial_current_density),
//! [`loop_current_density`](crate::assembly::magnetostatic3d::loop_current_density))
//! only cover their own fixture geometries. This module builds `J` for an
//! **arbitrary named current path** on a tagged mesh: a conductor volume
//! (tet subset) plus a *source* and a *sink* terminal face (triangle sets on
//! the conductor's boundary).
//!
//! # Construction: a P1 conduction solve
//!
//! On the conductor tets only, solve the steady conduction problem
//!
//! ```text
//!   ∇·(σ ∇φ) = 0   in the conductor,
//!   φ = 1 on the source face,  φ = 0 on the sink face,
//!   σ ∂φ/∂n = 0 on the rest of the conductor surface (insulated),
//! ```
//!
//! with the generic P1 Dirichlet-electrode machinery of
//! [`crate::assembly::electrostatic::assemble_electrostatic`] (the per-tet
//! relative conductivity `σ_r` goes where `ε_r` goes; no volume source), then
//! take `J = −σ∇φ` per tet via
//! [`crate::assembly::electrostatic::recover_e_field`]. P1 gives a constant
//! gradient per tet — exactly the piecewise-constant `J` the Nédélec RHS
//! assumes, so no projection step is needed. `J` is finally rescaled to a
//! **unit net current** (1 A).
//!
//! # Discrete solenoidality (why no divergence cleaning is needed)
//!
//! [`check_solenoidal`](crate::assembly::magnetostatic3d::check_solenoidal)
//! evaluates, at each free node `n`, the discrete divergence
//! `d_n = Σ_{e∋n} ±b_e = ∫ ∇λ_n · J dV` (the Whitney edge functions of the
//! edges around `n`, summed with their incidence signs, are exactly `∇λ_n`).
//! With `J = −σ∇φ` this is `−∫ σ ∇λ_n·∇φ dV`, i.e. minus the P1 conduction
//! residual at node `n`, which is zero at every node that is free in the
//! conduction solve. The source/sink nodes (Dirichlet in the conduction
//! solve) are where current legitimately enters / leaves the path and are
//! returned as the terminal's `exempt_nodes`. So the excitation passes the
//! compatibility gate *by Galerkin orthogonality* — this is verified
//! numerically (to solver round-off) by the unit tests below rather than
//! assumed.
//!
//! # Net current
//!
//! The same identity defines the **Galerkin (weak) current** through the
//! source face: with `χ_src = Σ_{n∈src} λ_n`,
//! `I = −∫ ∇χ_src · J dV`, and the sink-side counterpart
//! `I_sink = +∫ ∇χ_sink · J dV` equals it exactly (the free-node residuals
//! vanish and `Σ_n ∇λ_n = 0`). That exact source/sink balance is why the weak
//! current is the normalisation; the geometric face flux
//! `∫_src J·n̂ dA` (via
//! [`crate::assembly::electrostatic::flux_into_incident_tets`]) is reported
//! as an independent cross-check. It agrees exactly for a uniform `J` and to
//! discretisation order otherwise.
//!
//! # Scope (v1) and deferrals
//!
//! * **Open paths only.** Source and sink must be distinct, node-disjoint
//!   faces. A topologically **closed** loop (a ring with no terminals) has no
//!   source/sink pair: driving it needs a cut surface carrying a potential
//!   *jump* (an EMF / cohomology-cut boundary condition), which is not
//!   implemented. It is not faked here.
//! * **Terminals must be grounded in the magnetostatic solve, on the same
//!   connected PEC component.** The source/sink nodes carry the net current
//!   into and out of the domain, so the curl-curl problem is only consistent
//!   if the PEC wall (the return conductor) absorbs it — the coax end caps
//!   joined by the outer shield are the canonical example.
//!   [`ungrounded_nodes`] reports terminal nodes that are not on the wall at
//!   all; a terminal face floating in the interior would need an explicit
//!   return path (a lumped gap source), which is out of scope.
//!
//! # Solvability: net current per grounded component
//!
//! Touching the PEC wall is necessary but **not sufficient**. The kernel of
//! the PEC-constrained discrete curl also contains `∇χ_k`, where `χ_k` is
//! the P1 indicator of one connected component `k` of the **grounded node
//! set**: the nodes of the constrained (PEC) edges of the system, i.e. the
//! edges with `interior_mask == false`, connected through those same
//! edges. (In the CLI the constrained edges are exactly the edges of the
//! PEC triangles plus the path terminal-contact triangles, so the
//! components can equally be computed from those triangles with
//! [`triangle_node_components`].) `∇χ_k` has zero tangential trace on
//! every PEC edge, so it is an admissible field, and the source is
//! compatible only if the **net current into every grounded component is
//! zero**. For an open path, that means the source and sink must touch the
//! **same connected component** of the PEC wall. `check_solenoidal` cannot
//! see this, because it skips every constrained node, and the tree-cotree
//! gauge makes the inconsistent system nonsingular, so an unbalanced
//! excitation yields a meaningless (typically non-SPD) inductance matrix
//! rather than a solve failure. [`grounded_components`] builds the
//! components and [`check_grounded_balance`] enforces the per-component
//! balance; call it once per path.
//! * **`σ_r`** only matters for *heterogeneous* conductors (current sharing
//!   between sub-regions of different conductivity). For a homogeneous path
//!   the shape of `J` is independent of the conductivity value.

use std::collections::{BTreeSet, HashMap};

use crate::assembly::electrostatic::{
    Electrode, ElectrostaticError, assemble_electrostatic, face_to_tet_map,
    flux_into_incident_tets, recover_e_field, tet_bary_grads,
};
use crate::assembly::magnetostatic3d::{CurrentTerminal, Magnetostatic3dSystem, tet_signed_volume};
use crate::mesh::TetMesh;

/// Error surfaced by the open-path current construction.
#[derive(Debug, Clone, PartialEq)]
pub enum CurrentPathError {
    /// Input length mismatch against the mesh (conductor mask, `σ_r`).
    ShapeMismatch(String),
    /// The conductor tet set is empty.
    EmptyConductor(String),
    /// A source or sink terminal face has no triangles.
    EmptyTerminal(String),
    /// A terminal triangle is not a face of any conductor tet.
    TerminalNotOnConductor(String),
    /// The source and sink faces share a node (a short / a closed loop
    /// with no gap) — the conduction potential would be double-valued.
    TerminalsOverlap(String),
    /// A per-tet conductivity on the conductor is not finite and positive.
    BadConductivity(String),
    /// The conduction solve failed (e.g. a conductor component with no
    /// terminal, left floating and singular).
    Conduction(ElectrostaticError),
    /// The source and sink are not connected through the conductor: the
    /// net current is zero / not finite.
    NoCurrent(String),
    /// A connected component of the grounded (PEC) node set receives a
    /// non-zero net current: the source and sink touch different PEC
    /// components (see [`check_grounded_balance`]).
    UnbalancedGround(String),
}

impl std::fmt::Display for CurrentPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "current path shape mismatch: {s}"),
            Self::EmptyConductor(s) => write!(f, "current path {s}: conductor has no tets"),
            Self::EmptyTerminal(s) => write!(f, "current path terminal {s} has no triangles"),
            Self::TerminalNotOnConductor(s) => {
                write!(f, "current path terminal not on the conductor: {s}")
            }
            Self::TerminalsOverlap(s) => write!(f, "current path source/sink overlap: {s}"),
            Self::BadConductivity(s) => write!(f, "current path conductivity: {s}"),
            Self::Conduction(e) => write!(f, "current path conduction solve failed: {e}"),
            Self::NoCurrent(s) => write!(f, "current path carries no current: {s}"),
            Self::UnbalancedGround(s) => {
                write!(
                    f,
                    "current path does not close through one PEC component: {s}"
                )
            }
        }
    }
}

impl std::error::Error for CurrentPathError {}

impl From<ElectrostaticError> for CurrentPathError {
    fn from(e: ElectrostaticError) -> Self {
        CurrentPathError::Conduction(e)
    }
}

/// An open current path's excitation plus its measurement diagnostics.
#[derive(Debug, Clone)]
pub struct OpenPathCurrent {
    /// The ready-to-use terminal: `j` normalised to a **1 A** Galerkin net
    /// current (`current = 1.0`), `exempt_nodes` = source ∪ sink nodes.
    pub terminal: CurrentTerminal,
    /// Source-face node indices (into the full mesh).
    pub source_nodes: Vec<u32>,
    /// Sink-face node indices (into the full mesh).
    pub sink_nodes: Vec<u32>,
    /// Galerkin current leaving through the sink for the normalised `j`
    /// (A). Equals `terminal.current` to solver round-off — the discrete
    /// conservation certificate.
    pub sink_current: f64,
    /// Geometric face flux `∫_src J·n̂ dA` into the conductor through the
    /// source triangles for the normalised `j` (A) — an independent
    /// cross-check of the 1 A normalisation.
    pub source_face_flux: f64,
    /// Geometric face flux out of the conductor through the sink triangles
    /// for the normalised `j` (A).
    pub sink_face_flux: f64,
    /// Number of conductor tets.
    pub n_conductor_tets: usize,
}

/// Build the per-tet current density of an **open** current path.
///
/// * `mesh` — the full tetrahedral mesh (the magnetostatic domain).
/// * `name` — terminal name (carried on the returned [`CurrentTerminal`]).
/// * `conductor` — per-tet membership mask (`true` = conducting), length
///   `mesh.n_tets()`.
/// * `sigma_r` — per-tet relative conductivity, length `mesh.n_tets()`;
///   only conductor tets are read (must be finite and `> 0` there). Pass
///   all-ones for a homogeneous conductor.
/// * `source`, `sink` — terminal triangles (node triples into `mesh.nodes`),
///   each a face of some conductor tet. Current flows from `source` to
///   `sink`.
///
/// Returns the excitation with `J` normalised to a 1 A Galerkin net current
/// (see the module docs) and `J = 0` outside the conductor.
///
/// # Errors
///
/// See [`CurrentPathError`].
pub fn open_path_current(
    mesh: &TetMesh,
    name: &str,
    conductor: &[bool],
    sigma_r: &[f64],
    source: &[[u32; 3]],
    sink: &[[u32; 3]],
) -> Result<OpenPathCurrent, CurrentPathError> {
    let n_tets = mesh.n_tets();
    if conductor.len() != n_tets {
        return Err(CurrentPathError::ShapeMismatch(format!(
            "conductor mask length {} != tet count {n_tets}",
            conductor.len()
        )));
    }
    if sigma_r.len() != n_tets {
        return Err(CurrentPathError::ShapeMismatch(format!(
            "sigma_r length {} != tet count {n_tets}",
            sigma_r.len()
        )));
    }
    let cond_tets: Vec<usize> = (0..n_tets).filter(|&t| conductor[t]).collect();
    if cond_tets.is_empty() {
        return Err(CurrentPathError::EmptyConductor(name.to_string()));
    }
    if let Some(&t) = cond_tets
        .iter()
        .find(|&&t| !(sigma_r[t].is_finite() && sigma_r[t] > 0.0))
    {
        return Err(CurrentPathError::BadConductivity(format!(
            "{name}: sigma_r[{t}] = {} must be finite and > 0",
            sigma_r[t]
        )));
    }
    if source.is_empty() {
        return Err(CurrentPathError::EmptyTerminal(format!("{name} (source)")));
    }
    if sink.is_empty() {
        return Err(CurrentPathError::EmptyTerminal(format!("{name} (sink)")));
    }

    // Every terminal triangle must be a face of a conductor tet.
    let face_to_tet = face_to_tet_map(mesh, Some(conductor));
    for (role, tris) in [("source", source), ("sink", sink)] {
        let missing = tris
            .iter()
            .filter(|tri| !face_to_tet.contains_key(&sorted3(**tri)))
            .count();
        if missing > 0 {
            return Err(CurrentPathError::TerminalNotOnConductor(format!(
                "{name}: {missing} of {} {role} triangles are not faces of a conductor tet",
                tris.len()
            )));
        }
    }
    let node_set = |tris: &[[u32; 3]]| -> Vec<u32> {
        tris.iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<u32>>()
            .into_iter()
            .collect()
    };
    let source_nodes = node_set(source);
    let sink_nodes = node_set(sink);
    {
        let src: BTreeSet<u32> = source_nodes.iter().copied().collect();
        let shared = sink_nodes.iter().filter(|n| src.contains(n)).count();
        if shared > 0 {
            return Err(CurrentPathError::TerminalsOverlap(format!(
                "{name}: source and sink share {shared} node(s); an open path needs \
                 node-disjoint terminal faces"
            )));
        }
    }

    // Compact conductor sub-mesh (only the conductor carries the
    // conduction problem; outside it σ = 0 and the P1 rows would vanish).
    let mut local_of: HashMap<u32, u32> = HashMap::new();
    let mut sub_nodes: Vec<[f64; 3]> = Vec::new();
    let mut sub_tets: Vec<[u32; 4]> = Vec::with_capacity(cond_tets.len());
    for &t in &cond_tets {
        let tet = mesh.tets[t];
        let mut lt = [0u32; 4];
        for (slot, &g) in lt.iter_mut().zip(tet.iter()) {
            *slot = *local_of.entry(g).or_insert_with(|| {
                sub_nodes.push(mesh.nodes[g as usize]);
                (sub_nodes.len() - 1) as u32
            });
        }
        sub_tets.push(lt);
    }
    let sub = TetMesh {
        nodes: sub_nodes,
        tets: sub_tets,
        physical_groups: Default::default(),
    };
    let sigma_sub: Vec<f64> = cond_tets.iter().map(|&t| sigma_r[t]).collect();
    let rho = vec![0.0_f64; sub.n_tets()];
    let to_local = |nodes: &[u32]| -> Vec<u32> { nodes.iter().map(|g| local_of[g]).collect() };
    let src_local = to_local(&source_nodes);
    let sink_local = to_local(&sink_nodes);
    let electrode = Electrode {
        name: format!("{name}/source"),
        nodes: src_local.clone(),
        voltage: 1.0,
    };
    let sys = assemble_electrostatic(&sub, &sigma_sub, &rho, &[electrode], &sink_local)?;
    let phi = sys.solve()?;
    // J = σ_r E = −σ_r ∇φ (arbitrary units; normalised below).
    let e_sub = recover_e_field(&sub, &phi);

    // Galerkin currents: I_src = −∫∇χ_src·J, I_sink = +∫∇χ_sink·J.
    let mut in_src = vec![false; sub.n_nodes()];
    for &n in &src_local {
        in_src[n as usize] = true;
    }
    let mut in_sink = vec![false; sub.n_nodes()];
    for &n in &sink_local {
        in_sink[n as usize] = true;
    }
    let mut j_sub: Vec<[f64; 3]> = Vec::with_capacity(sub.n_tets());
    let (mut i_src, mut i_sink) = (0.0_f64, 0.0_f64);
    // Element-stiffness scale `max σ V |∇λ|²` (≈ σ h): the conduction
    // current of any connected path is at least ~σ h / (elements along the
    // path), while a disconnected source/sink leaves only round-off.
    let mut k_scale = 0.0_f64;
    for (k, tet) in sub.tets.iter().enumerate() {
        let coords = tet.map(|v| sub.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let vol = tet_signed_volume(&coords).abs();
        let s = sigma_sub[k];
        for g in &grads {
            k_scale = k_scale.max(s * vol * (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]));
        }
        let j = [s * e_sub[k][0], s * e_sub[k][1], s * e_sub[k][2]];
        for (p, &v) in tet.iter().enumerate() {
            let g = grads[p];
            let flux = vol * (g[0] * j[0] + g[1] * j[1] + g[2] * j[2]);
            if in_src[v as usize] {
                i_src -= flux;
            }
            if in_sink[v as usize] {
                i_sink += flux;
            }
        }
        j_sub.push(j);
    }
    if !(i_src.is_finite() && i_src.abs() > 1e-9 * k_scale) {
        return Err(CurrentPathError::NoCurrent(format!(
            "{name}: Galerkin source current {i_src:e} (source and sink not connected \
             through the conductor?)"
        )));
    }
    let inv = 1.0 / i_src;
    let mut j_full = vec![[0.0_f64; 3]; n_tets];
    for (k, &t) in cond_tets.iter().enumerate() {
        j_full[t] = [j_sub[k][0] * inv, j_sub[k][1] * inv, j_sub[k][2] * inv];
    }
    let source_face_flux = flux_into_incident_tets(mesh, &j_full, source, &face_to_tet, |_| 1.0);
    let sink_face_flux = -flux_into_incident_tets(mesh, &j_full, sink, &face_to_tet, |_| 1.0);

    let mut exempt: Vec<u32> = source_nodes.iter().chain(&sink_nodes).copied().collect();
    exempt.sort_unstable();
    Ok(OpenPathCurrent {
        terminal: CurrentTerminal {
            name: name.to_string(),
            j: j_full,
            current: 1.0,
            exempt_nodes: exempt,
        },
        source_nodes,
        sink_nodes,
        sink_current: i_sink * inv,
        source_face_flux,
        sink_face_flux,
        n_conductor_tets: cond_tets.len(),
    })
}

/// The nodes of `nodes` that are **not** on the magnetostatic system's PEC
/// wall (not an endpoint of any PEC edge, i.e. an edge with
/// `interior_mask == false`).
///
/// An open path's source/sink nodes must all be grounded: the net current
/// enters and leaves the domain there, so the curl-curl source problem is
/// only consistent if the PEC wall (the return conductor) absorbs it. A
/// non-empty result means the excitation's discrete divergence at those
/// nodes is not balanced by anything and the inductance would be wrong.
///
/// An empty result is **necessary but not sufficient**: the source and
/// sink must also lie on the *same connected component* of the PEC wall —
/// check that with [`check_grounded_balance`].
pub fn ungrounded_nodes(sys: &Magnetostatic3dSystem, nodes: &[u32]) -> Vec<u32> {
    let mut grounded = vec![false; sys.n_nodes];
    for (e, &keep) in sys.interior_mask.iter().enumerate() {
        if !keep {
            grounded[sys.edges[e][0] as usize] = true;
            grounded[sys.edges[e][1] as usize] = true;
        }
    }
    nodes
        .iter()
        .copied()
        .filter(|&n| !grounded[n as usize])
        .collect()
}

/// Connected components of a node subset, as dense labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeComponents {
    /// Per node (length = total node count): `Some(k)` with
    /// `k < count` if the node is in the subset, `None` otherwise.
    /// Labels are assigned in ascending order of each component's smallest
    /// node index.
    pub label: Vec<Option<u32>>,
    /// Number of components.
    pub count: usize,
}

impl NodeComponents {
    /// The distinct component labels touched by `nodes` (ascending);
    /// nodes outside the subset are ignored.
    pub fn touched(&self, nodes: &[u32]) -> Vec<u32> {
        let set: BTreeSet<u32> = nodes
            .iter()
            .filter_map(|&n| self.label.get(n as usize).copied().flatten())
            .collect();
        set.into_iter().collect()
    }
}

/// Connected components of the graph spanned by `edges` on `n_nodes`
/// nodes. The subset is the set of edge endpoints; two nodes are connected
/// iff a chain of `edges` joins them. Isolated nodes (no incident edge)
/// get `None`.
///
/// # Panics
///
/// If an edge endpoint is `>= n_nodes`.
pub fn edge_node_components(
    n_nodes: usize,
    edges: impl IntoIterator<Item = [u32; 2]>,
) -> NodeComponents {
    let mut parent: Vec<u32> = (0..n_nodes as u32).collect();
    let mut member = vec![false; n_nodes];
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            let up = parent[parent[x as usize] as usize];
            parent[x as usize] = up;
            x = up;
        }
        x
    }
    for [a, b] in edges {
        member[a as usize] = true;
        member[b as usize] = true;
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            parent[hi as usize] = lo;
        }
    }
    let mut label = vec![None; n_nodes];
    let mut root_label: HashMap<u32, u32> = HashMap::new();
    for n in 0..n_nodes {
        if !member[n] {
            continue;
        }
        let r = find(&mut parent, n as u32);
        let next = root_label.len() as u32;
        label[n] = Some(*root_label.entry(r).or_insert(next));
    }
    NodeComponents {
        label,
        count: root_label.len(),
    }
}

/// Connected components of the node set of one or more triangle lists,
/// connected through the triangle edges. With the PEC triangles plus every
/// path's source/sink contact triangles, this is exactly
/// [`grounded_components`] of the system assembled from the corresponding
/// `pec_interior_mask_from_triangles` mask.
pub fn triangle_node_components(n_nodes: usize, triangle_lists: &[&[[u32; 3]]]) -> NodeComponents {
    edge_node_components(
        n_nodes,
        triangle_lists
            .iter()
            .flat_map(|l| l.iter())
            .flat_map(|t| [[t[0], t[1]], [t[1], t[2]], [t[0], t[2]]]),
    )
}

/// Connected components of the magnetostatic system's **grounded node
/// set**: the endpoints of the constrained (PEC) edges
/// (`interior_mask == false`), connected through those edges. Each
/// component `k` contributes a kernel direction `∇χ_k` of the constrained
/// curl-curl (see the module docs), so each must receive zero net current.
pub fn grounded_components(sys: &Magnetostatic3dSystem) -> NodeComponents {
    edge_node_components(
        sys.n_nodes,
        sys.interior_mask
            .iter()
            .zip(&sys.edges)
            .filter(|(keep, _)| !**keep)
            .map(|(_, e)| *e),
    )
}

/// Net current each grounded component injects into the domain for the
/// per-tet current density `j`: `I_k = −∫ ∇χ_k · J dV`, indexed by
/// component label of `components`. For an open path whose source and sink
/// lie on the same component this is `0` everywhere (to round-off); a
/// source on component `a` and a sink on component `b ≠ a` give
/// `I_a = +I`, `I_b = −I`.
///
/// # Panics
///
/// If `j.len() != mesh.n_tets()` or `components.label.len() != mesh.n_nodes()`.
pub fn component_net_currents(
    mesh: &TetMesh,
    j: &[[f64; 3]],
    components: &NodeComponents,
) -> Vec<f64> {
    assert_eq!(j.len(), mesh.n_tets(), "j length != tet count");
    assert_eq!(
        components.label.len(),
        mesh.n_nodes(),
        "component labels length != node count"
    );
    let mut net = vec![0.0_f64; components.count];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let jt = j[t];
        if jt == [0.0; 3] {
            continue;
        }
        if tet.iter().all(|&v| components.label[v as usize].is_none()) {
            continue;
        }
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let vol = tet_signed_volume(&coords).abs();
        for (p, &v) in tet.iter().enumerate() {
            if let Some(k) = components.label[v as usize] {
                let g = grads[p];
                net[k as usize] -= vol * (g[0] * jt[0] + g[1] * jt[1] + g[2] * jt[2]);
            }
        }
    }
    net
}

/// Enforce the **per-component current balance** for one path's
/// excitation on the assembled system: every connected component of the
/// grounded node set ([`grounded_components`]) must receive a net current
/// `|I_k| ≤ tol · |terminal.current|`. Returns the worst relative
/// imbalance on success.
///
/// For an open path this is the statement that the source and sink touch
/// the **same connected PEC component**. With several paths, call it once
/// per path (each excitation must balance on its own).
///
/// # Errors
///
/// [`CurrentPathError::UnbalancedGround`] naming the offending components,
/// or [`CurrentPathError::ShapeMismatch`] on a mesh/system mismatch.
pub fn check_grounded_balance(
    sys: &Magnetostatic3dSystem,
    mesh: &TetMesh,
    terminal: &CurrentTerminal,
    tol: f64,
) -> Result<f64, CurrentPathError> {
    if sys.n_nodes != mesh.n_nodes() || terminal.j.len() != mesh.n_tets() {
        return Err(CurrentPathError::ShapeMismatch(format!(
            "{}: system/mesh/current size mismatch",
            terminal.name
        )));
    }
    let comps = grounded_components(sys);
    let net = component_net_currents(mesh, &terminal.j, &comps);
    let scale = terminal.current.abs().max(f64::MIN_POSITIVE);
    // NaN-aware: a non-finite imbalance counts as a violation.
    let over = |v: f64| {
        let r = v.abs() / scale;
        r.is_nan() || r > tol
    };
    let worst = net.iter().fold(0.0_f64, |w, v| {
        let r = v.abs() / scale;
        if r.is_nan() { f64::NAN } else { w.max(r) }
    });
    if over(worst) || net.iter().any(|&v| over(v)) {
        let bad: Vec<String> = net
            .iter()
            .enumerate()
            .filter(|(_, v)| over(**v))
            .map(|(k, v)| format!("component {k}: {v:+.3e} A"))
            .collect();
        return Err(CurrentPathError::UnbalancedGround(format!(
            "{}: {} of {} grounded (PEC) components receive a net current ({}) for a \
             {:.3e} A path current — the source and sink must touch the same connected \
             PEC component (add the PEC surface that joins them)",
            terminal.name,
            bad.len(),
            comps.count,
            bad.join(", "),
            terminal.current
        )));
    }
    Ok(worst)
}

#[inline]
fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_components_label_and_count() {
        // Two chains {0-1-2} and {4-5}; node 3 isolated.
        let c = edge_node_components(6, [[0, 1], [2, 1], [5, 4]]);
        assert_eq!(c.count, 2);
        assert_eq!(
            c.label,
            vec![Some(0), Some(0), Some(0), None, Some(1), Some(1)]
        );
        assert_eq!(c.touched(&[3]), Vec::<u32>::new());
        assert_eq!(c.touched(&[2, 5, 0]), vec![0, 1]);
    }

    #[test]
    fn triangle_components_join_through_shared_edges_and_vertices() {
        // Triangles A=(0,1,2), B=(2,3,4) share vertex 2 -> one component;
        // C=(5,6,7) is separate.
        let a = [[0, 1, 2], [2, 3, 4]];
        let b = [[5, 6, 7]];
        let c = triangle_node_components(9, &[&a, &b]);
        assert_eq!(c.count, 2);
        assert_eq!(c.touched(&[0, 4]), vec![0]);
        assert_eq!(c.touched(&[0, 7]), vec![0, 1]);
        assert_eq!(c.label[8], None);
        // A bridging list merges them.
        let bridge = [[4, 5, 8]];
        assert_eq!(triangle_node_components(9, &[&a, &b, &bridge]).count, 1);
    }
}
