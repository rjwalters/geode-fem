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
//! * **Terminals must be grounded in the magnetostatic solve.** The
//!   source/sink nodes carry the net current into and out of the domain, so
//!   the curl-curl problem is only consistent if they lie on the PEC wall
//!   (the return conductor) — the coax end caps are the canonical example.
//!   [`ungrounded_nodes`] reports violations; a terminal face floating in the
//!   interior would need an explicit return path (a lumped gap source),
//!   which is out of scope.
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

#[inline]
fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}
