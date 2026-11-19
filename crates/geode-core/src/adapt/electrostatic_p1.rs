//! Explicit residual a-posteriori error estimator for the **scalar P1
//! electrostatic** problem (issue #1765, parent #1481).
//!
//! # Problem
//!
//! The discrete P1 solution `φ_h` of the electrostatic solve
//! ([`crate::assembly::electrostatic::assemble_electrostatic`]) satisfies
//!
//! ```text
//!   −∇·( ε₀ ε_r ∇φ_h ) = ρ        (weakly, per tet T: ε_T = ε₀ ε_r[t])
//! ```
//!
//! with conductor electrodes and a ground held at prescribed potentials
//! (Dirichlet) and everything else carrying the homogeneous natural
//! (Neumann) condition `D·n_out = 0`, where `D = −ε₀ε_r ∇φ_h` is the
//! displacement field (SI: `D` in C/m²). Given the nodal potential `phi`
//! and the same per-tet `eps_r` / `rho` inputs the assembler accepted,
//! [`estimate_electrostatic_p1`] returns a per-tet indicator `η_T²` and
//! the global `η = (Σ_T η_T²)^{1/2}` identifying which tets carry the
//! discretization error.
//!
//! # Estimator
//!
//! Per tet `T` with faces `F ⊂ ∂T` (Verfürth-style residual weights):
//!
//! ```text
//!   η_T² = h_T² ‖ρ_T‖²_{L²(T)}                    cell (volume residual)
//!        + ½ Σ_{F interior} h_F ‖J_F‖²_{L²(F)}    face jumps (½ to each side)
//!        + Σ_{F natural}    h_F ‖J_F‖²_{L²(F)}    boundary flux residual
//! ```
//!
//! with `h_T` the tet diameter (longest edge, m), `h_F` the face diameter
//! (longest edge of the face, m), and `‖·‖_{L²}` over the tet volume
//! (`V_T`) / face area (`A_F`). `ρ_T` and `J_F` are piecewise constant,
//! so `‖ρ_T‖²_{L²(T)} = ρ_T² V_T` and `‖J_F‖²_{L²(F)} = J_F² A_F`.
//! Each interior face is counted **once** and its squared contribution is
//! split evenly between its two incident tets; a natural boundary face
//! goes fully to its single tet.
//!
//! **Cell term.** For P1, `∇φ_h` is constant per tet, and with a constant
//! per-tet `ε_T` the volume divergence `∇·(ε_T ∇φ_h)` vanishes element by
//! element. The signed cell residual is therefore exactly the prescribed
//! charge `ρ_T` (C/m³) — the strong residual of `−∇·(ε∇φ) = ρ` on `T`.
//!
//! **Sign conventions (kept signed for the goal estimator, #1766).**
//!
//! - *Cell*: the signed term is `ρ_T` (C/m³); positive `ρ_T` is a
//!   positive strong residual of `−∇·(ε∇φ) = ρ`.
//! - *Interior face F* shared by `T⁻`/`T⁺` (the **lower/higher tet
//!   index**, so the orientation is deterministic): with the unit normal
//!   `n̂` pointing **from `T⁻` to `T⁺`**,
//!
//!   ```text
//!   J_F = ε_{T⁺} ∇φ_h|_{T⁺}·n̂ − ε_{T⁻} ∇φ_h|_{T⁻}·n̂      (C/m²)
//!       = D_{T⁻}·n̂ − D_{T⁺}·n̂ = −[[D·n̂]],
//!   ```
//!
//!   i.e. the jump of the normal flux of `−D`: the **negative** of the
//!   net outward displacement flux density of a thin Gauss (pillbox)
//!   box straddling `F` (`J_F = −(D_{T⁺} − D_{T⁻})·n̂`, the interface
//!   charge the discrete solution implies up to that sign). `J_F = 0`
//!   is the continuity of normal displacement the exact solution
//!   satisfies across a material interface.
//! - *Natural boundary face F* of tet `T`: with `n̂` the **outward**
//!   unit normal (away from `T`'s off-face vertex),
//!   `J_F = −ε_T ∇φ_h|_T·n̂ = D·n_out` (C/m²); the homogeneous natural
//!   condition is `J_F = 0`.
//!
//! **Prescribed faces are skipped.** A face whose three nodes all lie in
//! **one conductor group** — the ground set, or a single electrode's node
//! set ([`prescribed_faces`]) — carries no residual contribution (the
//! trace of the discrete solution there is the conductor's prescribed
//! data). This covers the electrode/ground surfaces and also tagged
//! **internal conductor sheets** (faces interior to the mesh whose nodes
//! all belong to one conductor). A face whose Dirichlet nodes come from
//! *different* conductors is **not** prescribed: it is not part of any
//! conductor surface, and its P1 trace merely interpolates between the
//! conductors' node values, so its jump/flux is a genuine residual term.
//! A face with one or two Dirichlet nodes is likewise not skipped: the
//! free node's test function is nonzero there.
//!
//! **Units.** `ρ_T` in C/m³, `J_F` in C/m², `h_T`, `h_F` in m, `A_F` in
//! m², `V_T` in m³, `ε₀` in F/m. Then `η_T²` and `η²` carry C²/m and
//! `η` carries C·m^{−1/2}. The diameter weights are the issue-prescribed
//! ε-independent form; `ε₀ ε_r` enters only through `J_F` itself.
//!
//! **Global estimate.** Every `η_T²` is a sum of squares (nonnegative),
//! and `η = sqrt(Σ_T η_T²)`, the root of the sum of the nonnegative
//! indicators ([`P1ElectrostaticResidual::eta_t2`]).
//!
//! **Signed terms before norms.** [`P1ElectrostaticResidual`] exposes the
//! signed `ρ_T` per tet and the signed `J_F` per face (with the two tet
//! indices and the orientation above, the face area and `h_F`) *before*
//! any squaring/norm, so the goal estimator (#1766) can form
//! `Σ ∫ ρ_T z_h + Σ ∫ J_F z_h` against its adjoint solution. Goal
//! derivatives, adjoint solves, loop control and CLI output are out of
//! scope here.

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

use crate::assembly::electrostatic::{EPS_0, Electrode, recover_e_field};
use crate::mesh::TetMesh;
use crate::vec3::{cross3 as cross, dot3 as dot, norm3, scale3 as scale, sorted3, sub3 as sub};

/// Error surfaced by [`estimate_electrostatic_p1`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectrostaticP1Error {
    /// Input length mismatch (`phi` against the node count; per-tet
    /// `eps_r` / `rho` against the tet count).
    ShapeMismatch(String),
    /// Non-manifold adjacency: a face is incident to **more than two**
    /// tets, so it has no unique two-sided neighbor orientation.
    /// Reported with the face's sorted node ids.
    NonManifoldFace(String),
}

impl std::fmt::Display for ElectrostaticP1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "electrostatic P1 estimator shape mismatch: {s}"),
            Self::NonManifoldFace(s) => {
                write!(f, "electrostatic P1 estimator non-manifold face: {s}")
            }
        }
    }
}

impl std::error::Error for ElectrostaticP1Error {}

/// A signed face residual term `J_F` (see [module docs](self) for the
/// sign convention), one entry per **non-prescribed** face of the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceJump {
    /// The face's node ids, sorted ascending (canonical face key).
    pub nodes: [u32; 3],
    /// Signed flux jump `J_F` (C/m²): `ε_{T⁺}∇φ|_{T⁺}·n̂ − ε_{T⁻}∇φ|_{T⁻}·n̂`
    /// on an interior face (`n̂` from `T⁻` to `T⁺`), `−ε_T ∇φ|_T·n_out` on a
    /// natural boundary face.
    pub jump: f64,
    /// Face area `A_F` (m²).
    pub area: f64,
    /// Face diameter `h_F` — the face's longest edge (m).
    pub h: f64,
    /// `T⁻`: on an interior face the lower-index incident tet (the one
    /// `n̂` points away from); on a boundary face its single tet.
    pub minus: usize,
    /// `T⁺` on an interior face (higher-index tet, the one `n̂` points
    /// toward); `None` on a natural boundary face.
    pub plus: Option<usize>,
}

impl FaceJump {
    /// `true` for an interior face (two incident tets, `n̂` from `minus`
    /// to `plus`); `false` for a natural boundary face.
    pub fn is_interior(&self) -> bool {
        self.plus.is_some()
    }
}

/// Output of [`estimate_electrostatic_p1`].
#[derive(Debug, Clone, PartialEq)]
pub struct P1ElectrostaticResidual {
    /// Per-tet squared indicator `η_T²` (C²/m, nonnegative), parallel to
    /// `mesh.tets`. The face terms are accumulated in the sorted
    /// [`P1ElectrostaticResidual::faces`] order (cell term last), so the
    /// values are bitwise-reproducible across runs despite `HashMap`'s
    /// per-process iteration order.
    pub eta_t2: Vec<f64>,
    /// Global estimate `η = (Σ_T η_T²)^{1/2}` (C·m^{−1/2}).
    pub eta: f64,
    /// Signed cell residual `ρ_T` (C/m³) per tet — the strong residual of
    /// `−∇·(ε∇φ) = ρ` for P1 with per-tet-constant `ε` — before any
    /// squaring/norm, for the goal estimator (#1766). Parallel to
    /// `mesh.tets`.
    pub cell_rho: Vec<f64>,
    /// Signed face terms, one per non-prescribed face, sorted by
    /// [`FaceJump::nodes`] for determinism — the same order the face
    /// shares are accumulated into `eta_t2` in, so the indicators are
    /// bitwise-reproducible across runs.
    pub faces: Vec<FaceJump>,
}

/// Local face table of a P1 tet: the three vertices *on* each face
/// (same table as [`crate::assembly::electrostatic::face_to_tet_map`]).
const TET_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

/// The prescribed (Dirichlet-data) face keys of `mesh`: every tet face
/// whose three nodes all belong to **one conductor group** — the ground
/// set, or a single electrode's node set.
///
/// This is the canonical prescribed-face identity of the P1 residual
/// estimator and the goal estimator ([`crate::adapt::goal_peak_field`]):
/// on such a face the discrete solution's trace is the conductor's
/// prescribed data (an equipotential surface, an electrode's boundary
/// skin, or an internal conductor sheet). A face whose Dirichlet nodes
/// come from *different* conductors is **not** prescribed — it is not
/// part of any conductor surface, so its flux jump is a genuine residual
/// term. Faces with a free node are never prescribed.
pub fn prescribed_faces(
    mesh: &TetMesh,
    electrodes: &[Electrode],
    ground: &[u32],
) -> BTreeSet<[u32; 3]> {
    let groups: Vec<BTreeSet<u32>> = std::iter::once(ground.iter().copied().collect())
        .chain(
            electrodes
                .iter()
                .map(|el| el.nodes.iter().copied().collect()),
        )
        .collect();
    let mut keys = BTreeSet::new();
    for tet in &mesh.tets {
        for f in &TET_FACES {
            let key = sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]);
            if groups.iter().any(|g| key.iter().all(|v| g.contains(v))) {
                keys.insert(key);
            }
        }
    }
    keys
}

/// Compute the per-tet residual indicators and the signed cell/face
/// residual terms of the scalar P1 electrostatic solve.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh.
/// * `phi` — nodal potential (V), length `mesh.n_nodes()` (the solved
///   `ElectrostaticSystem` solution, scattered over all nodes).
/// * `eps_r` — per-tet relative permittivity (dimensionless), length
///   `mesh.n_tets()`; the per-tet permittivity is `ε_T = ε₀·eps_r[t]`,
///   the same SI convention as the assembly.
/// * `rho` — per-tet volume charge density (C/m³), length
///   `mesh.n_tets()`.
/// * `electrodes` / `ground` — the Dirichlet node sets
///   (`assemble_electrostatic`'s); together they define the prescribed
///   faces ([`prescribed_faces`]: all three nodes in one conductor
///   group) that are excluded from the residual.
///
/// `∇φ_h` is recovered per tet as `−E` with
/// [`recover_e_field`].
///
/// # Errors
///
/// [`ElectrostaticP1Error::ShapeMismatch`] on any length mismatch;
/// [`ElectrostaticP1Error::NonManifoldFace`] when a face is incident to
/// more than two tets (checked in every build profile).
pub fn estimate_electrostatic_p1(
    mesh: &TetMesh,
    phi: &[f64],
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<P1ElectrostaticResidual, ElectrostaticP1Error> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if phi.len() != n_nodes {
        return Err(ElectrostaticP1Error::ShapeMismatch(format!(
            "phi length {} != node count {n_nodes}",
            phi.len()
        )));
    }
    if eps_r.len() != n_tets {
        return Err(ElectrostaticP1Error::ShapeMismatch(format!(
            "eps_r length {} != tet count {n_tets}",
            eps_r.len()
        )));
    }
    if rho.len() != n_tets {
        return Err(ElectrostaticP1Error::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }

    // Per-tet ∇φ_h (V/m): E = −∇φ from the shared recovery, sign flipped back.
    let grad_phi: Vec<[f64; 3]> = recover_e_field(mesh, phi)
        .into_iter()
        .map(|e| scale(e, -1.0))
        .collect();

    // The prescribed-face identity shared with the goal estimator: all
    // three nodes in one conductor group (ground or a single electrode).
    let prescribed = prescribed_faces(mesh, electrodes, ground);

    // Per-tet diameter h_T (longest edge, m) and volume V_T (m³).
    let mut h_tet = vec![0.0_f64; n_tets];
    let mut vol_tet = vec![0.0_f64; n_tets];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let mut h = 0.0_f64;
        for i in 0..4 {
            for j in i + 1..4 {
                h = h.max(norm3(sub(coords[i], coords[j])));
            }
        }
        h_tet[t] = h;
        vol_tet[t] = (dot(
            sub(coords[1], coords[0]),
            cross(sub(coords[2], coords[0]), sub(coords[3], coords[0])),
        ))
        .abs()
            / 6.0;
    }

    // Face → (T⁻, T⁺) adjacency: sorted-triple key, **both** incident
    // tets kept (the assembler's `face_to_tet_map` keeps only one).
    let mut face_tets: HashMap<[u32; 3], (usize, Option<usize>)> =
        HashMap::with_capacity(n_tets * 2);
    for (t, tet) in mesh.tets.iter().enumerate() {
        for f in &TET_FACES {
            let key = sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]);
            match face_tets.entry(key) {
                Entry::Vacant(slot) => {
                    slot.insert((t, None));
                }
                Entry::Occupied(mut slot) => {
                    if slot.get().1.is_some() {
                        // Checked in every build profile: a third tet on
                        // this face would silently overwrite the second
                        // neighbor and drop its share of the face term.
                        return Err(ElectrostaticP1Error::NonManifoldFace(format!(
                            "face {key:?} is incident to more than two tets"
                        )));
                    }
                    slot.get_mut().1 = Some(t);
                }
            }
        }
    }

    let mut eta_t2 = vec![0.0_f64; n_tets];
    let mut faces = Vec::new();

    for (key, (t_minus, t_plus)) in &face_tets {
        let tm = *t_minus;
        // Prescribed face: all three nodes in one conductor group ⇒ the
        // trace there is prescribed data ⇒ no residual contribution
        // (electrode/ground surfaces, and internal conductor sheets,
        // whose faces are interior).
        if prescribed.contains(key) {
            continue;
        }
        let pa = mesh.nodes[key[0] as usize];
        let pb = mesh.nodes[key[1] as usize];
        let pc = mesh.nodes[key[2] as usize];
        let n2 = scale(cross(sub(pb, pa), sub(pc, pa)), 0.5); // area-weighted normal
        let area = norm3(n2);
        let n_unit = scale(n2, 1.0 / area);
        let h_face = norm3(sub(pb, pa))
            .max(norm3(sub(pc, pa)))
            .max(norm3(sub(pc, pb)));
        match t_plus {
            Some(tp) => {
                // Interior face: orient n̂ from T⁻ toward T⁺ (toward T⁺'s
                // off-face vertex).
                let v_p = mesh.tets[*tp]
                    .iter()
                    .copied()
                    .find(|v| !key.contains(v))
                    .expect("interior tet must have a vertex off the shared face");
                let n = if dot(n_unit, sub(mesh.nodes[v_p as usize], pa)) < 0.0 {
                    scale(n_unit, -1.0)
                } else {
                    n_unit
                };
                // Signed jump: ε_{T⁺}∇φ|_{T⁺}·n̂ − ε_{T⁻}∇φ|_{T⁻}·n̂.
                let j =
                    EPS_0 * (eps_r[*tp] * dot(grad_phi[*tp], n) - eps_r[tm] * dot(grad_phi[tm], n));
                // Counted once, split ½/½ between the two incident tets;
                // accumulated in the second pass over the sorted `faces`
                // below so the addition order is process-independent.
                faces.push(FaceJump {
                    nodes: *key,
                    jump: j,
                    area,
                    h: h_face,
                    minus: tm,
                    plus: Some(*tp),
                });
            }
            None => {
                // Natural boundary face: n̂ outward (away from T⁻'s
                // off-face vertex); residual is the full flux, not a jump.
                let v_m = mesh.tets[tm]
                    .iter()
                    .copied()
                    .find(|v| !key.contains(v))
                    .expect("boundary face's tet must have a vertex off the face");
                let n = if dot(n_unit, sub(mesh.nodes[v_m as usize], pa)) > 0.0 {
                    scale(n_unit, -1.0)
                } else {
                    n_unit
                };
                let j = -EPS_0 * eps_r[tm] * dot(grad_phi[tm], n);
                faces.push(FaceJump {
                    nodes: *key,
                    jump: j,
                    area,
                    h: h_face,
                    minus: tm,
                    plus: None,
                });
            }
        }
    }
    faces.sort_by_key(|f| f.nodes);

    // Face-jump term: `½ h_F J² A_F` to each incident tet (interior) or
    // the full `h_F J² A_F` (natural). Accumulated in this second pass
    // over the **sorted** `faces` — not inside the map-iteration loop
    // above — because a `HashMap`'s iteration order follows the
    // per-process `RandomState` seed, which would make the addition
    // order — and hence `eta_t2`/`eta` in the last ULP — vary run to
    // run. The per-face share expressions mirror the loop above exactly.
    for f in &faces {
        match f.plus {
            Some(tp) => {
                let share = 0.5 * f.h * f.jump * f.jump * f.area;
                eta_t2[f.minus] += share;
                eta_t2[tp] += share;
            }
            None => {
                eta_t2[f.minus] += f.h * f.jump * f.jump * f.area;
            }
        }
    }

    // Cell term: h_T² ‖ρ_T‖²_{L²(T)} = h_T² · ρ_T² · V_T.
    for t in 0..n_tets {
        eta_t2[t] += h_tet[t] * h_tet[t] * rho[t] * rho[t] * vol_tet[t];
    }

    let eta = eta_t2.iter().sum::<f64>().sqrt();
    Ok(P1ElectrostaticResidual {
        eta_t2,
        eta,
        cell_rho: rho.to_vec(),
        faces,
    })
}
