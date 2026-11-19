//! Goal-oriented (dual-weighted-residual) error estimate for the
//! box-averaged peak-field observable (issue #1766, parent #1481).
//!
//! The observable [`crate::postproc::peak_field::peak_field`] reports the
//! maximum — over candidate window centres — of the volume-weighted mean
//! `|E|` over a small box. This module attaches to that value a **signed
//! first-order estimate of the discretization error**, localized to one
//! signed contribution per coarse tet, via an enriched adjoint on a
//! conforming refinement of the primal mesh.
//!
//! # Construction
//!
//! Let `J(φ)` be the observable at the *winning* window `B` (the box
//! [`crate::postproc::peak_field::PeakField::window`] the scan selected —
//! the lowest-index candidate on a tie), with denominator `V_cov` the
//! covered volume the scan reported:
//!
//! ```text
//!   J(φ) = Σ_T (|T ∩ B| / V_cov) · |E_T| ,    E_T = −∇φ|_T .
//! ```
//!
//! **Linearization (goal derivative).** `B` and `V_cov` are held fixed
//! during differentiation: the max is non-smooth, and the estimate is the
//! derivative of the *selected* branch. Along a perturbation `v` of the
//! fine P1 potential, `dE_t(v) = −∇v|_t` on each fine tet, so
//!
//! ```text
//!   dJ[v] = − Σ_{fine tets t} (|t ∩ B| / V_cov) · Ê_T · ∇v|_t ,
//!   Ê_T = E_T / |E_T|  on the coarse parent T of t .
//! ```
//!
//! *Zero-field subgradient convention*: `Ê_T = 0` when `|E_T| = 0`, so a
//! zero-field tet contributes nothing to the goal derivative. This is the
//! zero element of the subdifferential of `|·|` at `0`; the true
//! one-sided directional derivatives there are `±|dE_t|`, so the estimate
//! is the symmetric (average) one.
//!
//! *Limitation*: if refinement moves the maximizing window to a different
//! candidate, the true observable is `max_B J_B(φ)` and is non-smooth
//! exactly when the argmax switches; this estimate linearizes the
//! currently-selected branch only and does not bound that switching.
//!
//! **Enriched adjoint.** The same-space adjoint is useless here: a discrete
//! adjoint `z_h` of the *coarse* space has defect `z_h − I_h z_h = 0`, and
//! the residual functional vanishes on the whole coarse space by Galerkin
//! orthogonality (`ℓ(v_h) − a(φ_h, v_h) = 0`), so a same-space adjoint
//! yields a zero estimate identically. One homogeneous-Dirichlet P1
//! adjoint is therefore solved on a conforming refinement of the primal
//! mesh ([`crate::adapt::refine`]): the material coefficients ride the
//! parent-tet map ([`crate::adapt::refine::Refined::parent_of_tet`]), the
//! prescribed surfaces are carried onto the refinement (every coarse
//! Dirichlet node stays pinned, and new fine nodes lying on an
//! all-Dirichlet coarse face are pinned too), and the goal derivative
//! above is the load. The adjoint defect is
//!
//! ```text
//!   w = z_f − P(z_f restricted to the coarse nodes) ,
//! ```
//!
//! with `P` the exact P1 nodal prolongation
//! ([`crate::adapt::refine::Refined::node_prolongation`]) — the enrichment
//! of `z_f` over its coarse interpolant.
//!
//! **Signed estimate.** With the residual conventions of
//! [`crate::adapt::electrostatic_p1`] (signed `ρ_T` and `J_F` before
//! norms), integration by parts gives, for any continuous piecewise-linear
//! `v` vanishing on the prescribed surface,
//!
//! ```text
//!   ℓ(v) − a(φ_h, v) = Σ_T ∫_T ρ_T v + Σ_F ∫_F J_F v ,
//! ```
//!
//! and the DWR identity gives the signed error estimate of
//! `J(reference) − J(coarse)` (error sign: reference minus coarse)
//!
//! ```text
//!   η = Σ_T ∫_T ρ_T w + Σ_F ∫_F J_F w .
//! ```
//!
//! `w` vanishes exactly on the prescribed surface (the adjoint is pinned
//! to zero there and so is its coarse interpolant), which is why the
//! prescribed faces the residual estimator skips stay skipped. Localization
//! ([`localize_signed_residual`]): the cell term integrates `ρ_T` of the
//! coarse parent against `w` on each child tet and is assigned to the
//! parent; each interior coarse face term is split equally between its two
//! coarse tets; a natural face term goes wholly to its sole owner. The
//! per-tet terms sum to the global estimate
//! ([`PeakFieldGoalEstimate::per_tet`]).
//!
//! # What is not here
//!
//! Adaptive-loop policy, marking consumption of the per-tet terms, P2
//! elements, CLI output, H(curl) goals and the effectivity study
//! (issue #1767). The P1 residual estimator is
//! [`crate::adapt::electrostatic_p1`].

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

use faer::Mat;

use crate::adapt::electrostatic_p1::{
    ElectrostaticP1Error, P1ElectrostaticResidual, estimate_electrostatic_p1,
};
use crate::adapt::refine::{RefineError, RefineOpts, Refined};
use crate::assembly::electrostatic::{
    Electrode, ElectrostaticError, assemble_electrostatic, recover_e_field, tet_bary_grads,
};
use crate::mesh::{TaggedTetMesh, TetMesh};
use crate::postproc::peak_field::{PeakField, PeakFieldError, Region, peak_field};
use crate::postproc::sample::{Aabb, cell_overlap_weights};
use crate::vec3::{
    cross3 as cross, dot3 as dot, norm3 as norm, scale3 as scale, sorted3, sub3 as sub,
    tet_signed_volume,
};

/// Errors of the peak-field goal estimate.
#[derive(Debug, thiserror::Error)]
pub enum PeakFieldGoalError {
    /// Two inputs that must be parallel have different lengths, or a
    /// parent-tet index is out of range.
    #[error("peak-field goal estimate shape mismatch: {0}")]
    ShapeMismatch(String),
    /// The observable scan failed (empty region, bad window edge, bad
    /// reference field).
    #[error("peak-field goal estimate: {0}")]
    PeakField(#[from] PeakFieldError),
    /// The P1 residual estimator failed (input lengths, non-manifold face).
    #[error("peak-field goal estimate: {0}")]
    Residual(#[from] ElectrostaticP1Error),
    /// The conforming refinement failed.
    #[error("peak-field goal estimate: {0}")]
    Refine(#[from] RefineError),
    /// The adjoint assembly or factorization failed.
    #[error("peak-field goal estimate: adjoint failed: {0}")]
    Adjoint(#[from] ElectrostaticError),
    /// The mesh carries no Dirichlet node, so the adjoint has no
    /// homogeneous-Dirichlet space to solve in.
    #[error("peak-field goal estimate: the mesh has no electrode/ground (Dirichlet) node")]
    NoDirichlet,
    /// A fine face matched no coarse face of its parent's tet (a
    /// refinement/localization bug — report it).
    #[error("peak-field goal estimate: localization failed (a bug, please report): {0}")]
    Localization(String),
}

/// Signed goal estimate of the box-averaged peak-field observable
/// ([`peak_field_goal_estimate`]).
pub struct PeakFieldGoalEstimate {
    /// The coarse observable: the value this estimate is attached to
    /// (returned unchanged from
    /// [`crate::postproc::peak_field::peak_field`], including the winning
    /// [`PeakField::window`] the linearization holds fixed).
    pub peak: PeakField,
    /// Signed first-order estimate of `J(reference) − J(coarse)`
    /// (error sign: reference minus coarse), first order in the
    /// discretization error of the winning-window average.
    pub estimate: f64,
    /// One signed contribution per **coarse** tet (parallel to
    /// `mesh.tets`); [`PeakFieldGoalEstimate::per_tet`] sums to
    /// [`PeakFieldGoalEstimate::estimate`] exactly.
    pub per_tet: Vec<f64>,
    /// The fine-mesh adjoint defect `w = z_f − P(z_f|coarse)` (nodal on
    /// the refined mesh), exposed so a caller can re-assemble the weak
    /// residual `ℓ(w) − a(φ_h, w)` independently.
    pub defect: Vec<f64>,
    /// The refinement step the adjoint was solved on: the refined mesh,
    /// the parent-tet map and the nodal prolongation.
    pub refined: Refined,
}

/// Signed residual localization onto coarse tets (issue #1766).
///
/// Given the P1 residual terms of the coarse solve
/// ([`P1ElectrostaticResidual::cell_rho`] per coarse tet and
/// [`crate::adapt::electrostatic_p1::FaceJump`] per non-prescribed coarse
/// face) and a continuous piecewise-linear weight `w_fine` on the refined
/// mesh, returns `(per_tet, total)` with
///
/// ```text
///   per_tet[T] = ρ_T · Σ_{child tets t of T} ∫_t w
///              + (½ of each interior residual face's ∫_F w share;
///                 all of each natural face's share)
///   total      = Σ_T per_tet[T] = ℓ(w) − a(φ_h, w)
/// ```
///
/// the signed weak residual. The cell integrals use the P1 vertex mean ×
/// volume (exact for piecewise-linear `w`), and each coarse face's subface
/// integrals use the vertex mean × area; the interior-face split is ½/½
/// between [`crate::adapt::electrostatic_p1::FaceJump::minus`]/`plus`, a
/// natural face term goes wholly to its `minus` (sole) owner. Prescribed
/// coarse faces contribute nothing (they are absent from the residual
/// list, and the goal defect vanishes on them anyway).
///
/// # Errors
///
/// [`PeakFieldGoalError::ShapeMismatch`] on length mismatches or a
/// parent index out of range;
/// [`PeakFieldGoalError::Localization`] if a multi-parent fine face lies
/// on no coarse face of its parent tet (a refinement bug).
pub fn localize_signed_residual(
    coarse: &TetMesh,
    fine: &TetMesh,
    parent_of_tet: &[usize],
    residual: &P1ElectrostaticResidual,
    w_fine: &[f64],
) -> Result<(Vec<f64>, f64), PeakFieldGoalError> {
    let n_coarse_tets = coarse.n_tets();
    if parent_of_tet.len() != fine.n_tets() {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "parent_of_tet length {} != fine tet count {}",
            parent_of_tet.len(),
            fine.n_tets()
        )));
    }
    if w_fine.len() != fine.n_nodes() {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "w_fine length {} != fine node count {}",
            w_fine.len(),
            fine.n_nodes()
        )));
    }
    if residual.cell_rho.len() != n_coarse_tets {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "residual cell_rho length {} != coarse tet count {}",
            residual.cell_rho.len(),
            n_coarse_tets
        )));
    }
    for (t, &p) in parent_of_tet.iter().enumerate() {
        if p >= n_coarse_tets {
            return Err(PeakFieldGoalError::ShapeMismatch(format!(
                "parent_of_tet[{t}] = {p} is out of range ({n_coarse_tets} coarse tets)"
            )));
        }
    }

    let mut per_tet = vec![0.0_f64; n_coarse_tets];

    // Cell terms: ρ_parent ∫_t w = ρ_parent · (vertex mean of w) · vol_t.
    for (t, tet) in fine.tets.iter().enumerate() {
        let p = parent_of_tet[t];
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| fine.nodes[tet[i] as usize]);
        let vol = tet_signed_volume(&coords).abs();
        let mean_w = (w_fine[tet[0] as usize]
            + w_fine[tet[1] as usize]
            + w_fine[tet[2] as usize]
            + w_fine[tet[3] as usize])
            * 0.25;
        per_tet[p] += residual.cell_rho[p] * vol * mean_w;
    }

    // Face terms: every fine subface of a coarse residual face.
    let face_of: HashMap<[u32; 3], usize> = residual
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| (f.nodes, i))
        .collect();

    // Fine face → tets adjacency (sorted-triple key, both tets kept).
    let mut fine_faces: HashMap<[u32; 3], (usize, Option<usize>)> = HashMap::new();
    for (t, tet) in fine.tets.iter().enumerate() {
        for f in &TET_FACES {
            let key = sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]);
            match fine_faces.entry(key) {
                Entry::Vacant(slot) => {
                    slot.insert((t, None));
                }
                Entry::Occupied(mut slot) => {
                    if slot.get().1.is_some() {
                        return Err(PeakFieldGoalError::Localization(format!(
                            "fine face {key:?} is incident to more than two tets"
                        )));
                    }
                    slot.get_mut().1 = Some(t);
                }
            }
        }
    }

    let tol = mesh_diagonal(coarse) * 1e-9;
    for (key, (t_minus, t_plus)) in &fine_faces {
        let p = parent_of_tet[*t_minus];
        // A fine face shared by two children of the same coarse parent is
        // interior to it: no coarse face, no residual term.
        if let Some(tp) = t_plus
            && parent_of_tet[*tp] == p
        {
            continue;
        }
        // The face lies on exactly one coarse face of the parent tet.
        let ct = coarse.tets[p];
        let cface = (0..4)
            .map(|a| {
                sorted3([
                    ct[TET_FACES[a][0]],
                    ct[TET_FACES[a][1]],
                    ct[TET_FACES[a][2]],
                ])
            })
            .find(|c| {
                let tri: [[f64; 3]; 3] = std::array::from_fn(|i| coarse.nodes[c[i] as usize]);
                key.iter()
                    .all(|&v| point_on_triangle(fine.nodes[v as usize], &tri, tol))
            });
        let Some(cface) = cface else {
            return Err(PeakFieldGoalError::Localization(format!(
                "fine face {key:?} of parent tet {p} lies on no coarse face"
            )));
        };
        let Some(&fi) = face_of.get(&cface) else {
            continue; // prescribed coarse face: no residual term.
        };
        let fj = &residual.faces[fi];
        let pts: [[f64; 3]; 3] = std::array::from_fn(|i| fine.nodes[key[i] as usize]);
        let area = 0.5 * norm(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
        let mean_w =
            (w_fine[key[0] as usize] + w_fine[key[1] as usize] + w_fine[key[2] as usize]) / 3.0;
        let term = fj.jump * area * mean_w;
        // Interior coarse face: ½/½ between its two tets; natural face:
        // wholly to its sole owner (`minus`).
        per_tet[fj.minus] += 0.5 * term;
        match fj.plus {
            Some(tp) => per_tet[tp] += 0.5 * term,
            None => per_tet[fj.minus] += 0.5 * term,
        }
    }

    let total = per_tet.iter().sum();
    Ok((per_tet, total))
}

/// The goal derivative of the winning-window average, assembled as a
/// fine-mesh nodal load (issue #1766, guidance 2).
///
/// For every fine tet `t` with coarse parent `parent_of_tet[t] = T` and a
/// positive `|t ∩ window|`, each vertex basis function `v` receives
///
/// ```text
///   −(|t ∩ window| / volume_covered) · unit_e_coarse[T] · ∇v|_t ,
/// ```
///
/// the linearization of the box-averaged `|E|` with the window and the
/// covered-volume denominator held fixed. `unit_e_coarse[T]` must be the
/// coarse field's unit vector `E_T / |E_T|`; the zero vector encodes the
/// zero-field subgradient convention (that tet contributes nothing).
///
/// # Panics
///
/// Panics if `parent_of_tet` is not parallel to `fine.tets`, a parent
/// index is out of range for `unit_e_coarse`, or `volume_covered` is not
/// positive and finite (delegated input checks, the [`crate::postproc::sample`]
/// convention).
pub fn goal_load_fine(
    fine: &TetMesh,
    parent_of_tet: &[usize],
    unit_e_coarse: &[[f64; 3]],
    window: Aabb,
    volume_covered: f64,
) -> Vec<f64> {
    assert_eq!(
        parent_of_tet.len(),
        fine.n_tets(),
        "parent_of_tet length {} != fine tet count {}",
        parent_of_tet.len(),
        fine.n_tets()
    );
    assert!(
        volume_covered > 0.0 && volume_covered.is_finite(),
        "volume_covered must be positive and finite, got {volume_covered}"
    );

    // Exact fine-tet overlap weights of the fixed window (same clipping as
    // the averaging observable, ascending tet index).
    let mut weight = vec![0.0_f64; fine.n_tets()];
    for (ti, w) in cell_overlap_weights(fine, &window) {
        weight[ti] = w;
    }

    let mut g = vec![0.0_f64; fine.n_nodes()];
    for (t, tet) in fine.tets.iter().enumerate() {
        let w = weight[t];
        if w == 0.0 {
            continue;
        }
        let ue = unit_e_coarse
            .get(parent_of_tet[t])
            .unwrap_or_else(|| panic!("parent_of_tet[{t}] out of range for unit_e_coarse"));
        if ue == &[0.0_f64; 3] {
            continue; // zero-field subgradient convention.
        }
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| fine.nodes[tet[i] as usize]);
        let grads = tet_bary_grads(&coords);
        for (p, &v) in tet.iter().enumerate() {
            g[v as usize] -= (w / volume_covered) * dot(*ue, grads[p]);
        }
    }
    g
}

/// Signed goal-oriented error estimate of the box-averaged peak-field
/// observable (issue #1766).
///
/// Runs the observable scan ([`crate::postproc::peak_field::peak_field`])
/// on the coarse recovered field `E = −∇φ`, holds the winning window
/// fixed, refines the whole mesh conformingly once, solves one
/// homogeneous-Dirichlet P1 adjoint there with the goal derivative as the
/// load, and weights the signed coarse residual
/// ([`estimate_electrostatic_p1`]) by the enriched adjoint defect.
/// See the [module docs](self) for the construction and its conventions
/// (zero-field subgradient, tie branch, non-smooth argmax limitation).
///
/// # Arguments
///
/// * `mesh` — the coarse (primal) tetrahedral mesh.
/// * `phi` — the solved coarse nodal potential (V), length
///   `mesh.n_nodes()`.
/// * `eps_r` / `rho` — the coarse per-tet relative permittivity and
///   volume charge density the primal assembler accepted.
/// * `electrodes` / `ground` — the primal Dirichlet node sets; the adjoint
///   pins the same surfaces (plus the refinement nodes lying on them) to
///   zero.
/// * `region` / `window_edge` — the observable's candidate region and
///   window edge, as in [`crate::postproc::peak_field::peak_field`].
/// * `e_ref` — the enhancement reference forwarded to the observable
///   (only feeds [`PeakField::enhancement`]; the estimate never uses it).
///
/// # Errors
///
/// [`PeakFieldGoalError`] variants for input mismatches, the observable,
/// the residual estimator, the refinement, or the adjoint solve
/// ([`PeakFieldGoalError::NoDirichlet`] when the mesh pins no node at
/// all).
#[allow(clippy::too_many_arguments)]
pub fn peak_field_goal_estimate(
    mesh: &TetMesh,
    phi: &[f64],
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    region: &Region,
    window_edge: f64,
    e_ref: f64,
) -> Result<PeakFieldGoalEstimate, PeakFieldGoalError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if phi.len() != n_nodes {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "phi length {} != node count {n_nodes}",
            phi.len()
        )));
    }
    if eps_r.len() != n_tets {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "eps_r length {} != tet count {n_tets}",
            eps_r.len()
        )));
    }
    if rho.len() != n_tets {
        return Err(PeakFieldGoalError::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }

    // 1. The observable and its winning window (fixed from here on).
    let field_per_tet = recover_e_field(mesh, phi);
    let peak = peak_field(mesh, None, &field_per_tet, region, window_edge, e_ref)?;
    let window = peak.window;

    // 2. Covered-volume denominator of the scan, recomputed with the same
    //    clipping and visit order as [`sample_cell_average`] (bit-identical).
    let mut volume_covered = 0.0_f64;
    for (_, w) in cell_overlap_weights(mesh, &window) {
        volume_covered += w;
    }

    // 3. Unit field per coarse tet; zero row = the zero-field subgradient.
    let unit_e: Vec<[f64; 3]> = field_per_tet
        .iter()
        .map(|&e| {
            let m = norm(e);
            if m > 0.0 { scale(e, 1.0 / m) } else { [0.0; 3] }
        })
        .collect();

    // 4. One conforming refinement of the primal mesh (all tets marked).
    let tagged = TaggedTetMesh {
        mesh: mesh.clone(),
        tet_physical_tags: vec![0; n_tets],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    };
    let marked: Vec<usize> = (0..n_tets).collect();
    let refined = crate::adapt::refine::refine(&tagged, &marked, &RefineOpts::default())?;
    let fine = &refined.mesh.mesh;
    let parent_of_tet = &refined.parent_of_tet;

    // 5. Materials ride the parent-tet map onto the fine mesh.
    let eps_r_fine: Vec<f64> = parent_of_tet.iter().map(|&p| eps_r[p]).collect();

    // 6. Homogeneous-Dirichlet node set of the refinement.
    let dirichlet_fine = refined_dirichlet_nodes(mesh, fine, electrodes, ground);
    if dirichlet_fine.is_empty() {
        return Err(PeakFieldGoalError::NoDirichlet);
    }

    // 7. Goal derivative as a fine-mesh nodal load.
    let g = goal_load_fine(fine, parent_of_tet, &unit_e, window, volume_covered);

    // 8. Adjoint solve: one SPD P1 system on the refinement, load = goal
    //    derivative, prescribed nodes pinned to zero.
    let sys = assemble_electrostatic(
        fine,
        &eps_r_fine,
        &vec![0.0; fine.n_tets()],
        &[Electrode {
            name: "goal-adjoint-dirichlet".to_string(),
            nodes: dirichlet_fine.iter().copied().collect(),
            voltage: 0.0,
        }],
        &[],
    )?;
    use faer::linalg::solvers::Solve;
    let mut b_free = vec![0.0_f64; sys.n_free];
    for (gi, &gv) in g.iter().enumerate() {
        if let Some(fi) = sys.free_of_global[gi] {
            b_free[fi] = gv;
        }
    }
    let lu = sys.k.as_ref().sp_lu().map_err(|e| {
        PeakFieldGoalError::Adjoint(ElectrostaticError::Factorization(format!("{e:?}")))
    })?;
    let mut rhs = Mat::from_fn(sys.n_free, 1, |i, _| b_free[i]);
    lu.solve_in_place(rhs.as_mut());
    let mut z_f = vec![0.0_f64; fine.n_nodes()];
    for (gi, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            z_f[gi] = rhs[(*fi, 0)];
        }
    }

    // 9. Enriched adjoint defect w = z_f − P·(z_f restricted to coarse
    //    nodes; old nodes keep their indices).
    let mut pzc = vec![0.0_f64; fine.n_nodes()];
    {
        let p = refined.node_prolongation.as_ref();
        let cp = p.col_ptr();
        let rows = p.row_idx();
        let vals = p.val();
        for (j, &zcj) in z_f.iter().enumerate().take(n_nodes) {
            for k in cp[j]..cp[j + 1] {
                pzc[rows[k]] += vals[k] * zcj;
            }
        }
    }
    let defect: Vec<f64> = z_f.iter().zip(&pzc).map(|(&z, &p)| z - p).collect();

    // 10. Signed residual localization onto the coarse tets.
    let residual = estimate_electrostatic_p1(mesh, phi, eps_r, rho, electrodes, ground)?;
    let (per_tet, estimate) =
        localize_signed_residual(mesh, fine, parent_of_tet, &residual, &defect)?;

    Ok(PeakFieldGoalEstimate {
        peak,
        estimate,
        per_tet,
        defect,
        refined,
    })
}

// ----------------------------------------------------------------- helpers

/// Local P1 face table of a tet (vertices *on* each face) — the same table
/// as [`crate::assembly::electrostatic::face_to_tet_map`] and
/// [`crate::adapt::electrostatic_p1`].
const TET_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

/// Mesh extent (bounding-box diagonal), the length scale of geometric
/// tolerances.
fn mesh_diagonal(mesh: &TetMesh) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    norm(sub(hi, lo))
}

/// Whether `p` lies on the triangle `tri` (within an absolute `tol` on the
/// plane distance and on the barycentric coordinates). Refinement nodes
/// are exact edge midpoints, so points that geometrically lie on a coarse
/// face satisfy this at roundoff level.
fn point_on_triangle(p: [f64; 3], tri: &[[f64; 3]; 3], tol: f64) -> bool {
    let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    let nn = norm(n);
    if nn == 0.0 {
        return false;
    }
    if dot(sub(p, tri[0]), n).abs() > tol * nn {
        return false;
    }
    // Barycentric coordinates in the projection dropping the dominant
    // normal axis (never degenerate for a non-degenerate triangle).
    let axis = (0..3)
        .max_by(|&a, &b| n[a].abs().total_cmp(&n[b].abs()))
        .unwrap();
    let keep: [usize; 2] = if axis == 0 {
        [1, 2]
    } else if axis == 1 {
        [0, 2]
    } else {
        [0, 1]
    };
    let x0 = tri[0][keep[0]];
    let y0 = tri[0][keep[1]];
    let x1 = tri[1][keep[0]];
    let y1 = tri[1][keep[1]];
    let x2 = tri[2][keep[0]];
    let y2 = tri[2][keep[1]];
    let px = p[keep[0]];
    let py = p[keep[1]];
    let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    if denom == 0.0 {
        return false;
    }
    let l0 = ((y1 - y2) * (px - x2) + (x2 - x1) * (py - y2)) / denom;
    let l1 = ((y2 - y0) * (px - x2) + (x0 - x2) * (py - y2)) / denom;
    let l2 = 1.0 - l0 - l1;
    (-tol..=1.0 + tol).contains(&l0)
        && (-tol..=1.0 + tol).contains(&l1)
        && (-tol..=1.0 + tol).contains(&l2)
}

/// The homogeneous-Dirichlet node set of the refinement: every coarse
/// Dirichlet node (they keep their indices) plus every *new* fine node
/// lying on a prescribed coarse face — a face whose three coarse nodes are
/// all Dirichlet, the same prescribed-face rule
/// [`estimate_electrostatic_p1`] uses (covering electrode/ground surfaces
/// and internal conductor sheets). A node of that set is exactly a vertex
/// of the prescribed surface's subdivision, so the remaining nodes' hat
/// functions span the refined mesh's homogeneous-Dirichlet space.
fn refined_dirichlet_nodes(
    coarse: &TetMesh,
    fine: &TetMesh,
    electrodes: &[Electrode],
    ground: &[u32],
) -> BTreeSet<u32> {
    let dirichlet: BTreeSet<u32> = electrodes
        .iter()
        .flat_map(|el| el.nodes.iter().copied())
        .chain(ground.iter().copied())
        .collect();
    if dirichlet.is_empty() {
        return dirichlet;
    }
    let mut keys: BTreeSet<[u32; 3]> = BTreeSet::new();
    for tet in &coarse.tets {
        for f in &TET_FACES {
            let key = sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]);
            if key.iter().all(|v| dirichlet.contains(v)) {
                keys.insert(key);
            }
        }
    }
    let tol = mesh_diagonal(coarse) * 1e-9;
    let faces: Vec<[[f64; 3]; 3]> = keys
        .iter()
        .map(|k| std::array::from_fn(|i| coarse.nodes[k[i] as usize]))
        .collect();
    let mut pinned = dirichlet;
    for (i, &p) in fine.nodes.iter().enumerate().skip(coarse.n_nodes()) {
        if faces.iter().any(|f| point_on_triangle(p, f, tol)) {
            pinned.insert(i as u32);
        }
    }
    pinned
}
