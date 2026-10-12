//! Guaranteed **lower bounds** on an electrostatic capacitance from the
//! complementary (flux, `H(div)`) variational principle (issue #1062).
//!
//! The conforming potential solves of [`crate::assembly::electrostatic`] give
//! **upper** bounds: the capacitance is the minimum of the field energy over
//! potentials, so any conforming trial potential overestimates it. This module
//! gives the other side. It builds a displacement field `D` that is
//! admissible for the dual problem on the same tetrahedral mesh, and turns it
//! into a certified number `C_lower ≤ C`. Together they bracket the converged
//! capacitance of the meshed geometry: `C_lower ≤ C ≤ C_primal`.
//!
//! # The quantity
//!
//! The setting is the one of the transmon studies (#1051, #1056): conductors
//! are surface triangles (zero-thickness sheets inside the mesh, or faces on
//! its boundary), the outer boundary is natural (zero normal flux) except
//! where a conductor lies on it, and `ε` is piecewise constant per
//! tetrahedron, scalar or a symmetric positive-definite tensor. Exactly one
//! conductor is **driven** (potential `1`), any number are **floating**
//! (neutral, potential free) and the rest are **ground** (potential `0`).
//! The capacitance is
//!
//! ```text
//!   C = min { ∫ ε∇φ·∇φ : φ = 1 on the driven conductor, φ = 0 on ground,
//!                         φ = an arbitrary constant on each floating one }.
//! ```
//!
//! With one floating conductor `f` and the driven island `i` this is the
//! Schur complement `C_ii − C_if²/C_ff` of the Maxwell matrix: minimising
//! over the floating constant is exactly eliminating it. The primal solves
//! use that reduction (`C_Σ` of the transmon studies), and the minimum
//! characterisation is why it is still an upper bound for every conforming
//! space. Let `φ*` be the exact minimiser and `E* = −∇φ*`.
//!
//! # The certificate (Thomson's principle, made robust)
//!
//! Let `D` be any vector field that is a polynomial on each tetrahedron. On
//! the domain cut along the conductor sheets, integration by parts gives
//!
//! ```text
//!   ∫ D·E*  =  Σ_T ∫_T φ* div D  +  Σ_F ∫_F φ* q_F ,
//!   q_F     =  −Σ_sides D·n_out      (the flux leaving face F)
//! ```
//!
//! where the face sum runs over every mesh face: interior faces (where `q_F`
//! is the normal jump), boundary faces (one side) and the two sides of each
//! sheet face. On a conductor face `φ*` is the conductor's constant
//! potential, so those terms are `V_k Q_k` with `Q_k` the net charge of `D` on
//! conductor `k`. The driven conductor contributes `Q_driven` (`V = 1`) and
//! the ground nothing (`V = 0`). By the maximum principle `0 ≤ φ* ≤ 1`
//! everywhere, including on the floating conductors (truncating an admissible
//! potential to `[0, 1]` keeps it admissible, a floating constant stays a
//! constant, and does not raise its energy, so the unique minimiser is
//! already truncated). Every other term is therefore bounded by its absolute
//! value:
//!
//! ```text
//!   ∫ D·E*  ≥  N := Q_driven − Σ_floating |Q_f| − Σ_{non-conductor F} ∫_F |q_F|
//!                                               − Σ_T ∫_T |div D| .
//! ```
//!
//! Cauchy–Schwarz in the `ε`-weighted inner product gives
//! `∫ D·E* ≤ (∫ D·ε⁻¹D)^½ (∫ εE*·E*)^½ = (W_D)^½ C^½`, hence
//!
//! ```text
//!   C  ≥  N² / W_D       whenever N > 0 ,        W_D = ∫ D·ε⁻¹D .
//! ```
//!
//! This holds for **every** piecewise-polynomial `D`, so the bound does not
//! rely on how `D` was computed or on any solver tolerance: an inexact solve
//! or a floating conductor that is not exactly neutral only lowers `N`.
//! For an exactly admissible `D` (divergence-free in every tetrahedron,
//! continuous normal component across every non-conductor face, zero normal
//! component on the natural boundary, neutral on every floating conductor)
//! the penalty terms vanish and the bound is Thomson's principle,
//! `C ≥ Q²/W_D`, with equality for `D = Q ε E*`. The floating reduction
//! needs nothing special: a floating conductor is a sheet whose total charge
//! is zero but whose charge distribution is free, which is the dual
//! counterpart of a free constant potential. The bound is therefore a lower
//! bound on the reduced `C_ii − C_if²/C_ff` itself; no Schur-complement
//! inequality is involved. (A grounded feedline gives `C_ii ≥ C_Σ`, which is
//! not a lower bound on `C_Σ`; the mutation tests check that certifying a
//! grounded-feedline flux without the `|Q_f|` term violates the bracket.)
//!
//! [`certify_lower_bound`] evaluates every term with quadrature that is exact
//! for the polynomial degrees involved. The face and volume `L¹` norms are
//! bounded above by `|F|^½ ‖q_F‖_{L²(F)}` and `|T|^½ ‖div D‖_{L²(T)}`.
//!
//! **Rounding is not bounded.** The certificate is evaluated in plain `f64`,
//! **not interval arithmetic**. A worst-case estimate of the summation error
//! over the 3.3M-tet transmon meshes is about `1e-10` relative (typically
//! far less). Callers must round the result down to a much coarser grid: the
//! transmon study floors each per-mesh bound at `1e-8` relative (6 decimals
//! in fF) and each family bracket at `1e-4` (2 decimals), so rounding cannot
//! reach a reported digit.
//!
//! # Two admissible fluxes
//!
//! Both live in the Raviart–Thomas space `RT_k` (`k ≥ 0`) on the **cut**
//! mesh: the normal moments on a conductor face are independent on its two
//! sides, so the jump there (the surface charge) is free. Elsewhere the
//! space is the conforming `H(div)` space.
//!
//! - [`crouzeix_raviart_flux`]: the lowest-order mixed (RT0) solution of the
//!   dual problem, obtained without a saddle-point solve. For a source-free
//!   problem with piecewise-constant `ε`, Marini's identity says the RT0
//!   mixed flux is `−ε∇_h u` with `u` the nonconforming Crouzeix–Raviart
//!   (face-midpoint P1) solution: `∇_h u` is constant per tetrahedron, and
//!   the CR equation of each free face is exactly the continuity of the
//!   normal flux across it. The CR system is SPD and is solved with the same
//!   AMG-PCG as the primal problem; the floating constraint is one shared
//!   DOF per floating conductor, as in the primal. This is an independent
//!   dual solve that needs no primal solution.
//! - [`equilibrated_flux`]: the Prager–Synge / Braess–Schöberl equilibration
//!   of a primal P1 or P2 potential `φ_h`. With hat functions `ψ_a` (a
//!   partition of unity) and `σ_h = −ε∇φ_h`, solve on every vertex patch
//!   `ω_a` a small mixed problem
//!
//!   ```text
//!     min ‖σ_a − ψ_a σ_h‖_{ε⁻¹}   over σ_a ∈ RT_k(ω_a),
//!     div σ_a = ∇ψ_a·σ_h  in every patch tetrahedron,
//!     σ_a·n = 0 on ∂ω_a and on the natural boundary,
//!   ```
//!
//!   with the conductor faces through `a` cut. Then `σ = Σ_a σ_a` has
//!   `div σ = ∇(Σ_a ψ_a)·σ_h = 0`. A patch without conductor faces is
//!   solvable because `∫ ∇ψ_a·σ_h = −a(φ_h, ψ_a) = 0` (Galerkin
//!   orthogonality); a conductor patch deposits the charge `a(φ_h, ψ_a)` on
//!   its conductor faces, so a floating conductor receives
//!   `Σ_{a∈f} a(φ_h, ψ_a) = 0` exactly when `φ_h` satisfies the floating
//!   constraint. With an inexact primal solve the patch data are made
//!   compatible by removing their mean, and the certificate pays for the
//!   tiny resulting divergence. The flux error is of the order of the primal
//!   energy error, so the bracket width is a small multiple of the primal
//!   error (and for an exact solve `W_σ = C_h + ‖σ − σ_h‖²`, the
//!   Prager–Synge estimator).
//!
//! **Choice.** The equilibrated flux is the one used for the transmon
//! headline. It reuses the P2 solve that already gives the upper bound,
//! costs only independent dense patch problems (about 300 unknowns at `k = 1`),
//! and inherits the P2 accuracy, which no lowest-order dual can match on the
//! same mesh (RT0 converges like P1 conforming, an order slower in the
//! smooth part). The CR flux is kept as the independent check: it needs no
//! primal field, so the two constructions share nothing but the mesh and the
//! certificate.
//!
//! **What checks what.** Agreement between the CR and the equilibrated
//! bounds checks how the *flux* is built (equilibration, static
//! condensation, the RT basis). It does not check the certificate itself,
//! because both pass through the same [`FaceTopology`] and
//! [`certify_lower_bound`]. The certificate is checked against known values:
//! the stacked parallel plate (exact, where every bound collapses onto the
//! closed form), the coax (the analytic `2πε₀t/ln(b/a)`, bracketed with
//! certainty by inscribed and circumscribed polygon families), and
//! single-tet closed forms for each penalty term and for the energy at every
//! order `k ≤ MAX_K`.
//!
//! # Scope
//!
//! Volume conductors (tetrahedra inside a metal) are not supported: every
//! conductor is a triangle set. The bound is for the meshed polygonal
//! geometry, the same "converged" as the primal studies; refinement that
//! does not move the surfaces does not change it.

// Dense local linear algebra and quadrature loops index several arrays at
// once; iterator chains would obscure them.
#![allow(clippy::needless_range_loop)]

use crate::assembly::electrostatic::{
    AmgPcgOptions, AmgPcgReport, EPS_0, ElectrostaticError, amg_pcg_maxwell, tet_bary_grads,
};
use crate::mesh::TetMesh;
use faer::linalg::matmul::matmul;
use faer::sparse::{SparseColMat, Triplet};
use faer::{Accum, Mat, Par};

/// Marker for "no tetrahedron" / "no conductor" in [`FaceTopology`].
const NONE: u32 = u32::MAX;

/// Vertices on local face `f` of a tetrahedron (the face opposite vertex `f`).
const LOCAL_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

/// Largest supported Raviart–Thomas order.
pub const MAX_K: usize = 4;

/// Vertex patches per parallel work item (deterministic chunking).
const PATCH_CHUNK: usize = 512;

/// Tetrahedra / faces per parallel work item of the certificate.
const CERT_CHUNK: usize = 4096;

/// Per-tetrahedron relative permittivity.
#[derive(Clone, Copy, Debug)]
pub enum Permittivity<'a> {
    /// Scalar `ε_r`, one per tetrahedron.
    Scalar(&'a [f64]),
    /// Symmetric positive-definite `ε_r` tensor, one per tetrahedron.
    Tensor(&'a [[[f64; 3]; 3]]),
}

impl Permittivity<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Scalar(e) => e.len(),
            Self::Tensor(e) => e.len(),
        }
    }

    /// The `ε_r` matrix of tetrahedron `t` (a tensor is symmetrized; see
    /// `validate` for the tolerance it must already meet).
    fn matrix(&self, t: usize) -> [[f64; 3]; 3] {
        match self {
            Self::Scalar(e) => diag(e[t]),
            Self::Tensor(e) => {
                std::array::from_fn(|i| std::array::from_fn(|j| 0.5 * (e[t][i][j] + e[t][j][i])))
            }
        }
    }

    /// `ε_r⁻¹` of tetrahedron `t`.
    fn inverse(&self, t: usize) -> [[f64; 3]; 3] {
        match self {
            Self::Scalar(e) => diag(1.0 / e[t]),
            Self::Tensor(_) => inv3(&self.matrix(t)),
        }
    }

    fn validate(&self, n_tets: usize) -> Result<(), ElectrostaticError> {
        if self.len() != n_tets {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "permittivity length {} != tet count {n_tets}",
                self.len()
            )));
        }
        let bad = |t: usize| {
            ElectrostaticError::ShapeMismatch(format!(
                "permittivity of tet {t} is not finite, symmetric and positive definite"
            ))
        };
        for t in 0..n_tets {
            let m = self.matrix(t);
            let finite = m.iter().flatten().all(|x| x.is_finite());
            // A rotated material tensor is symmetric only to roundoff.
            let symmetric = match self {
                Self::Scalar(_) => true,
                Self::Tensor(e) => {
                    let scale = e[t].iter().flatten().fold(0.0_f64, |a, x| a.max(x.abs()));
                    (0..3).all(|i| (0..3).all(|j| (e[t][i][j] - e[t][j][i]).abs() <= 1e-12 * scale))
                }
            };
            let minors = [m[0][0], m[0][0] * m[1][1] - m[0][1] * m[1][0], det3(&m)];
            if !finite || !symmetric || minors.iter().any(|&x| x <= 0.0) {
                return Err(bad(t));
            }
        }
        Ok(())
    }
}

/// How a conductor enters the capacitance (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConductorRole {
    /// Held at potential `1`; exactly one conductor has this role.
    Driven,
    /// Neutral, with a free constant potential.
    Floating,
    /// Held at potential `0`.
    Ground,
}

/// A conductor of the dual problem: its surface triangles and role.
#[derive(Clone, Debug)]
pub struct DualConductor {
    /// Human-readable name.
    pub name: String,
    /// Role in the capacitance.
    pub role: ConductorRole,
    /// Surface triangles (node triples into `mesh.nodes`); each must be a
    /// face of the mesh, interior (a sheet) or on the boundary.
    pub triangles: Vec<[u32; 3]>,
}

/// Face table of a tetrahedral mesh with the conductor faces marked.
#[derive(Clone, Debug)]
pub struct FaceTopology {
    /// Unique faces as sorted vertex triples, in ascending order.
    pub faces: Vec<[u32; 3]>,
    /// Face index of each tetrahedron's local face `f` (opposite vertex `f`).
    pub tet_faces: Vec<[u32; 4]>,
    /// The one or two tetrahedra of each face (`u32::MAX` for none).
    pub face_tets: Vec<[u32; 2]>,
    /// Conductor index of each face (`u32::MAX` for a non-conductor face).
    pub face_conductor: Vec<u32>,
    /// Conductor roles, in the order given to [`FaceTopology::new`].
    pub roles: Vec<ConductorRole>,
    /// Conductor names, in the same order.
    pub names: Vec<String>,
}

impl FaceTopology {
    /// Build the face table and mark the conductor faces.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if there is not exactly one
    /// driven conductor or no ground conductor, if a conductor triangle is
    /// not a mesh face, or if a face belongs to two conductors;
    /// [`ElectrostaticError::EmptyElectrode`] for a conductor without
    /// triangles.
    pub fn new(mesh: &TetMesh, conductors: &[DualConductor]) -> Result<Self, ElectrostaticError> {
        let n_driven = conductors
            .iter()
            .filter(|c| c.role == ConductorRole::Driven)
            .count();
        if n_driven != 1 {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "dual bound needs exactly one driven conductor, got {n_driven}"
            )));
        }
        if !conductors.iter().any(|c| c.role == ConductorRole::Ground) {
            return Err(ElectrostaticError::ShapeMismatch(
                "dual bound needs a ground conductor".into(),
            ));
        }
        if let Some(c) = conductors.iter().find(|c| c.triangles.is_empty()) {
            return Err(ElectrostaticError::EmptyElectrode(c.name.clone()));
        }
        let mut keys: Vec<([u32; 3], u32)> = Vec::with_capacity(4 * mesh.n_tets());
        for (t, tet) in mesh.tets.iter().enumerate() {
            for (f, lf) in LOCAL_FACES.iter().enumerate() {
                keys.push((sorted3(lf.map(|l| tet[l])), (4 * t + f) as u32));
            }
        }
        par_sort(&mut keys);
        let mut faces = Vec::with_capacity(keys.len() / 2 + 1);
        let mut face_tets = Vec::with_capacity(keys.len() / 2 + 1);
        let mut tet_faces = vec![[NONE; 4]; mesh.n_tets()];
        let mut i = 0;
        while i < keys.len() {
            let mut j = i + 1;
            while j < keys.len() && keys[j].0 == keys[i].0 {
                j += 1;
            }
            if j - i > 2 {
                return Err(ElectrostaticError::ShapeMismatch(format!(
                    "face {:?} is shared by {} tets (non-manifold mesh)",
                    keys[i].0,
                    j - i
                )));
            }
            let id = faces.len() as u32;
            faces.push(keys[i].0);
            let mut pair = [NONE; 2];
            for (s, k) in keys[i..j].iter().enumerate() {
                let (t, f) = ((k.1 / 4) as usize, (k.1 % 4) as usize);
                pair[s] = t as u32;
                tet_faces[t][f] = id;
            }
            face_tets.push(pair);
            i = j;
        }
        drop(keys);
        let mut face_conductor = vec![NONE; faces.len()];
        for (ci, c) in conductors.iter().enumerate() {
            for tri in &c.triangles {
                let key = sorted3(*tri);
                let Ok(id) = faces.binary_search(&key) else {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "conductor {} triangle {tri:?} is not a mesh face",
                        c.name
                    )));
                };
                if face_conductor[id] != NONE && face_conductor[id] != ci as u32 {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "face {key:?} belongs to two conductors"
                    )));
                }
                face_conductor[id] = ci as u32;
            }
        }
        Ok(Self {
            faces,
            tet_faces,
            face_tets,
            face_conductor,
            roles: conductors.iter().map(|c| c.role).collect(),
            names: conductors.iter().map(|c| c.name.clone()).collect(),
        })
    }

    /// Number of faces.
    pub fn n_faces(&self) -> usize {
        self.faces.len()
    }

    fn driven(&self) -> usize {
        self.roles
            .iter()
            .position(|r| *r == ConductorRole::Driven)
            .expect("validated in FaceTopology::new")
    }
}

/// A Raviart–Thomas `RT_k` flux on a tetrahedral mesh, stored per
/// tetrahedron as coefficients of a fixed spanning basis of `RT_k(T)` in the
/// scaled local coordinate `ξ = (x − x_T)/h_T` (`x_T` the centroid, `h_T` the
/// longest edge). Each tetrahedron is independent, so the same storage holds
/// a field with jumps; [`certify_lower_bound`] measures them.
#[derive(Clone, Debug)]
pub struct RtFlux {
    /// Polynomial order `k` of `RT_k`.
    pub k: usize,
    /// `n_tets × dim(RT_k)` coefficients, tetrahedron-major.
    pub coeffs: Vec<f64>,
}

impl RtFlux {
    /// `dim RT_k(T) = (k+1)(k+2)(k+4)/2`.
    pub fn dim(k: usize) -> usize {
        (k + 1) * (k + 2) * (k + 4) / 2
    }
}

/// The certified lower bound and the terms it is built from (all in
/// `ε_r`-weighted mesh units, except `c_lower_f`).
#[derive(Clone, Debug)]
pub struct DualBound {
    /// The certified lower bound `ε₀ N² / W_D` in farads (mesh in metres),
    /// `0` if `N ≤ 0`.
    pub c_lower_f: f64,
    /// `W_D = ∫ D·ε_r⁻¹ D`.
    pub energy: f64,
    /// Net charge `Q_k` of `D` on each conductor (flux leaving its faces).
    pub charges: Vec<f64>,
    /// `Σ ∫_F |q_F|` bound over interior non-conductor faces (normal jumps).
    pub interior_jump_l1: f64,
    /// `Σ ∫_F |D·n|` bound over boundary non-conductor faces.
    pub boundary_flux_l1: f64,
    /// `Σ ∫_T |div D|` bound.
    pub divergence_l1: f64,
    /// `N = Q_driven − Σ|Q_floating| − (the three L¹ terms)`.
    pub numerator: f64,
}

/// The potential that a primal solve produced, with Dirichlet values in place
/// and the floating constraint applied (for example `φ_i − (C_fi/C_ff) φ_f`).
#[derive(Clone, Copy, Debug)]
pub enum PrimalPotential<'a> {
    /// Nodal P1 values, length `n_nodes`.
    P1(&'a [f64]),
    /// P2 values: vertices, then global edge midpoints ([`TetMesh::edges`]
    /// order), as [`crate::assembly::electrostatic::assemble_electrostatic_p2`].
    P2(&'a [f64]),
}

/// Report of [`crouzeix_raviart_flux`].
#[derive(Clone, Debug)]
pub struct CrouzeixRaviartReport {
    /// The CR energy-method capacitance `ε₀ (C_dd − C_df C_ff⁻¹ C_fd)` in
    /// farads. For an exact solve it equals the certified bound.
    pub c_cr_f: f64,
    /// Free CR (face) unknowns.
    pub n_free: usize,
    /// PCG convergence of the unit excitations.
    pub pcg: AmgPcgReport,
}

// ─────────────────────────────────────────────────────────────────────────
// Certificate
// ─────────────────────────────────────────────────────────────────────────

/// Certify a lower bound on the capacitance from any [`RtFlux`] (see the
/// module docs for the inequality and its proof).
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] on inconsistent sizes, an invalid
/// permittivity, `flux.k > MAX_K` or a non-finite result.
pub fn certify_lower_bound(
    mesh: &TetMesh,
    topo: &FaceTopology,
    eps: Permittivity<'_>,
    flux: &RtFlux,
) -> Result<DualBound, ElectrostaticError> {
    certify_impl(mesh, topo, eps, flux, Mutation::None)
}

/// Deliberate defects of the certificate, used only by the mutation tests to
/// show that each term is load-bearing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum Mutation {
    None,
    /// Drop the `|Q_floating|` term.
    IgnoreFloatingCharge,
    /// Drop the jump, boundary and divergence terms.
    IgnoreSpurious,
}

fn certify_impl(
    mesh: &TetMesh,
    topo: &FaceTopology,
    eps: Permittivity<'_>,
    flux: &RtFlux,
    mutation: Mutation,
) -> Result<DualBound, ElectrostaticError> {
    eps.validate(mesh.n_tets())?;
    if flux.k > MAX_K {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "certify_lower_bound supports k <= {MAX_K}"
        )));
    }
    let space = RtSpace::new(flux.k);
    let dim = space.dim;
    if flux.coeffs.len() != dim * mesh.n_tets() || topo.tet_faces.len() != mesh.n_tets() {
        return Err(ElectrostaticError::ShapeMismatch(
            "certify_lower_bound: flux or topology does not match the mesh".into(),
        ));
    }
    // Volume terms: energy and divergence, per deterministic chunk.
    let vol_parts = par_chunks(mesh.n_tets(), CERT_CHUNK, |range| {
        let mut energy = 0.0;
        let mut div_l1 = 0.0;
        let mut s = vec![[0.0; 3]; dim];
        let mut d = vec![0.0; dim];
        for t in range {
            let g = TetGeom::new(mesh, t);
            let c = &flux.coeffs[t * dim..(t + 1) * dim];
            let w_mat = eps.inverse(t);
            let (mut e_t, mut div2) = (0.0, 0.0);
            for (lam, w) in &space.tet_q {
                let xi = g.xi(&g.point(lam));
                space.eval(&xi, &mut s);
                space.div(&xi, g.h, &mut d);
                let v = combine(c, &s);
                e_t += w * dot(v, matvec(&w_mat, v));
                let dv: f64 = c.iter().zip(&d).map(|(a, b)| a * b).sum();
                div2 += w * dv * dv;
            }
            energy += g.vol * e_t;
            div_l1 += g.vol * div2.max(0.0).sqrt(); // |T|^½ (|T| mean)^½
        }
        (energy, div_l1)
    });
    let energy: f64 = vol_parts.iter().map(|p| p.0).sum();
    let divergence_l1: f64 = vol_parts.iter().map(|p| p.1).sum();

    // Face terms.
    let n_cond = topo.roles.len();
    let face_parts = par_chunks(topo.n_faces(), CERT_CHUNK, |range| {
        let mut charges = vec![0.0; n_cond];
        let (mut jump_l1, mut bnd_l1) = (0.0, 0.0);
        let mut s = vec![[0.0; 3]; dim];
        let mut q = vec![0.0; space.tri_q.len()];
        for fi in range {
            let fv = topo.faces[fi];
            let p = fv.map(|v| mesh.nodes[v as usize]);
            let (n, area) = unit_normal(&p);
            q.iter_mut().for_each(|x| *x = 0.0);
            for &t in topo.face_tets[fi].iter().filter(|&&t| t != NONE) {
                let t = t as usize;
                let g = TetGeom::new(mesh, t);
                let tet = mesh.tets[t];
                let opp = tet.iter().copied().find(|v| !fv.contains(v)).unwrap();
                // +1 when n_F points into this tet: then D·n_out = −D·n_F.
                let side = dot(n, sub(mesh.nodes[opp as usize], p[0])).signum();
                let c = &flux.coeffs[t * dim..(t + 1) * dim];
                for (qi, (lam, _)) in space.tri_q.iter().enumerate() {
                    let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                    space.eval(&g.xi(&x), &mut s);
                    q[qi] += side * dot(combine(c, &s), n);
                }
            }
            let int: f64 = space.tri_q.iter().zip(&q).map(|((_, w), v)| w * v).sum();
            let int2: f64 = space
                .tri_q
                .iter()
                .zip(&q)
                .map(|((_, w), v)| w * v * v)
                .sum();
            let l1 = area * int2.max(0.0).sqrt();
            match topo.face_conductor[fi] {
                NONE if topo.face_tets[fi][1] == NONE => bnd_l1 += l1,
                NONE => jump_l1 += l1,
                ci => charges[ci as usize] += area * int,
            }
        }
        (charges, jump_l1, bnd_l1)
    });
    let mut charges = vec![0.0; n_cond];
    let (mut interior_jump_l1, mut boundary_flux_l1) = (0.0, 0.0);
    for (c, j, b) in &face_parts {
        for (acc, v) in charges.iter_mut().zip(c) {
            *acc += v;
        }
        interior_jump_l1 += j;
        boundary_flux_l1 += b;
    }
    let mut numerator = charges[topo.driven()];
    if mutation != Mutation::IgnoreFloatingCharge {
        numerator -= topo
            .roles
            .iter()
            .zip(&charges)
            .filter(|(r, _)| **r == ConductorRole::Floating)
            .map(|(_, q)| q.abs())
            .sum::<f64>();
    }
    if mutation != Mutation::IgnoreSpurious {
        numerator -= interior_jump_l1 + boundary_flux_l1 + divergence_l1;
    }
    let c_lower_f = if numerator > 0.0 && energy > 0.0 {
        EPS_0 * numerator * numerator / energy
    } else {
        0.0
    };
    if !c_lower_f.is_finite() || !energy.is_finite() {
        return Err(ElectrostaticError::ShapeMismatch(
            "certify_lower_bound: non-finite result".into(),
        ));
    }
    Ok(DualBound {
        c_lower_f,
        energy,
        charges,
        interior_jump_l1,
        boundary_flux_l1,
        divergence_l1,
        numerator,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Crouzeix–Raviart (RT0 mixed) flux
// ─────────────────────────────────────────────────────────────────────────

/// The lowest-order mixed dual flux through the Crouzeix–Raviart identity
/// (module docs), stored as an [`RtFlux`] with `k = 0`.
///
/// Every conductor face is a pinned CR unknown (driven `1`, ground `0`); each
/// floating conductor's faces share one unknown, eliminated by the same
/// unit-excitation Schur reduction as the primal.
///
/// # Errors
///
/// As [`crate::assembly::electrostatic::extract_capacitance_amg_pcg`], and
/// [`ElectrostaticError::ShapeMismatch`] for an invalid permittivity or a
/// singular floating block.
pub fn crouzeix_raviart_flux(
    mesh: &TetMesh,
    topo: &FaceTopology,
    eps: Permittivity<'_>,
    opts: AmgPcgOptions,
) -> Result<(RtFlux, CrouzeixRaviartReport), ElectrostaticError> {
    eps.validate(mesh.n_tets())?;
    let nf = topo.n_faces();
    let mut free_of_global = vec![None; nf];
    let mut n_free = 0usize;
    for (fi, slot) in free_of_global.iter_mut().enumerate() {
        if topo.face_conductor[fi] == NONE {
            *slot = Some(n_free);
            n_free += 1;
        }
    }
    let mut full = Vec::with_capacity(16 * mesh.n_tets());
    let mut red = Vec::with_capacity(16 * mesh.n_tets());
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let vol = tet_volume(&coords);
        let e = eps.matrix(t);
        for i in 0..4 {
            let ei = matvec(&e, grads[i]);
            for j in 0..4 {
                // ∇ψ_F(i) = −3∇λ_i for the face opposite vertex i.
                let v = 9.0 * vol * dot(ei, grads[j]);
                let (a, b) = (topo.tet_faces[t][i] as usize, topo.tet_faces[t][j] as usize);
                full.push(Triplet::new(a, b, v));
                if let (Some(fa), Some(fb)) = (free_of_global[a], free_of_global[b]) {
                    red.push(Triplet::new(fa, fb, v));
                }
            }
        }
    }
    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(nf, nf, &full)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
    drop(full);
    let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
    drop(red);
    // Excitations: the driven conductor first, then each floating one.
    let driven = topo.driven();
    let floating: Vec<usize> = (0..topo.roles.len())
        .filter(|&c| topo.roles[c] == ConductorRole::Floating)
        .collect();
    let order: Vec<usize> = std::iter::once(driven)
        .chain(floating.iter().copied())
        .collect();
    let faces_of = |c: usize| -> Vec<usize> {
        (0..nf)
            .filter(|&f| topo.face_conductor[f] == c as u32)
            .collect()
    };
    let cond_dofs: Vec<Vec<usize>> = order.iter().map(|&c| faces_of(c)).collect();
    let ground_dofs: Vec<usize> = (0..topo.roles.len())
        .filter(|&c| topo.roles[c] == ConductorRole::Ground)
        .flat_map(faces_of)
        .collect();
    let names: Vec<String> = order.iter().map(|&c| topo.names[c].clone()).collect();
    let (cm, pcg, us) = amg_pcg_maxwell(
        &k,
        &k_full,
        &free_of_global,
        &names,
        &cond_dofs,
        &ground_dofs,
        opts,
    )?;
    let v = floating_potentials(&cm.c)?;
    let mut u = us[0].clone();
    for (vf, uf) in v.iter().zip(&us[1..]) {
        for (a, b) in u.iter_mut().zip(uf) {
            *a += vf * b;
        }
    }
    let c_cr = cm.c[0][0]
        + v.iter()
            .enumerate()
            .map(|(j, vf)| vf * cm.c[0][j + 1])
            .sum::<f64>();
    let dim = RtFlux::dim(0);
    let mut coeffs = vec![0.0; dim * mesh.n_tets()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let mut grad_u = [0.0; 3];
        for i in 0..4 {
            let ui = u[topo.tet_faces[t][i] as usize];
            for d in 0..3 {
                grad_u[d] -= 3.0 * ui * grads[i][d];
            }
        }
        let sigma = matvec(&eps.matrix(t), grad_u);
        // RT0 spanning order: e_x, e_y, e_z, then ξ.
        for d in 0..3 {
            coeffs[t * dim + d] = -sigma[d];
        }
    }
    Ok((
        RtFlux { k: 0, coeffs },
        CrouzeixRaviartReport {
            c_cr_f: EPS_0 * c_cr,
            n_free,
            pcg,
        },
    ))
}

/// Potentials of the floating conductors for a unit driven potential:
/// `v = −C_ff⁻¹ C_fd`, with the driven conductor at index 0 of `c`.
fn floating_potentials(c: &[Vec<f64>]) -> Result<Vec<f64>, ElectrostaticError> {
    let m = c.len() - 1;
    let mut a: Vec<f64> = (0..m * m).map(|ij| c[1 + ij / m][1 + ij % m]).collect();
    let mut b: Vec<f64> = (0..m).map(|i| -c[1 + i][0]).collect();
    if !cholesky(&mut a, m) {
        return Err(ElectrostaticError::ShapeMismatch(
            "floating-conductor block is not positive definite".into(),
        ));
    }
    chol_solve(&a, m, &mut b);
    Ok(b)
}

// ─────────────────────────────────────────────────────────────────────────
// Equilibrated flux (Prager–Synge / Braess–Schöberl)
// ─────────────────────────────────────────────────────────────────────────

/// Equilibrate a primal potential into an [`RtFlux`] of order `k` by
/// independent vertex-patch problems (module docs). `k = 1` matches P2 (the
/// divergence data `∇ψ_a·σ_h` is then represented exactly) and is more
/// than enough for P1.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] on inconsistent sizes, an invalid
/// permittivity, `k > MAX_K`, or a patch whose local mass matrix is not
/// positive definite (a degenerate tetrahedron).
pub fn equilibrated_flux(
    mesh: &TetMesh,
    topo: &FaceTopology,
    eps: Permittivity<'_>,
    primal: PrimalPotential<'_>,
    k: usize,
) -> Result<RtFlux, ElectrostaticError> {
    eps.validate(mesh.n_tets())?;
    if k > MAX_K {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "equilibrated_flux supports k <= {MAX_K}"
        )));
    }
    let tet_edges = match primal {
        PrimalPotential::P1(phi) => {
            if phi.len() != mesh.n_nodes() {
                return Err(ElectrostaticError::ShapeMismatch(
                    "P1 potential length != node count".into(),
                ));
            }
            None
        }
        PrimalPotential::P2(phi) => {
            let te = mesh.tet_edges();
            let n_edges = te
                .iter()
                .flatten()
                .map(|e| e.0 as usize + 1)
                .max()
                .unwrap_or(0);
            if phi.len() != mesh.n_nodes() + n_edges {
                return Err(ElectrostaticError::ShapeMismatch(
                    "P2 potential length != node + edge count".into(),
                ));
            }
            Some(te)
        }
    };
    if topo.tet_faces.len() != mesh.n_tets() {
        return Err(ElectrostaticError::ShapeMismatch(
            "topology does not match the mesh".into(),
        ));
    }
    let space = RtSpace::new(k);
    let dim = space.dim;
    // Vertex → tetrahedra (CSR).
    let n_nodes = mesh.n_nodes();
    let mut start = vec![0usize; n_nodes + 1];
    for tet in &mesh.tets {
        for &v in tet {
            start[v as usize + 1] += 1;
        }
    }
    for i in 0..n_nodes {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut vt = vec![0u32; start[n_nodes]];
    for (t, tet) in mesh.tets.iter().enumerate() {
        for &v in tet {
            vt[fill[v as usize]] = t as u32;
            fill[v as usize] += 1;
        }
    }
    drop(fill);
    let ctx = PatchCtx {
        mesh,
        topo,
        eps,
        primal,
        tet_edges: tet_edges.as_deref(),
        space: &space,
    };
    let mut coeffs = vec![0.0; dim * mesh.n_tets()];
    // Batches bound the memory of the per-patch results; chunk results are
    // added in chunk order, so the sum does not depend on the thread count.
    let batch = 64 * PATCH_CHUNK;
    let mut first = 0;
    while first < n_nodes {
        let last = (first + batch).min(n_nodes);
        let parts = par_chunks(last - first, PATCH_CHUNK, |range| {
            let mut out: Vec<(u32, Vec<f64>)> = Vec::new();
            for v in range {
                let a = first + v;
                let tets = &vt[start[a]..start[a + 1]];
                if tets.is_empty() {
                    continue;
                }
                match ctx.solve_patch(a as u32, tets) {
                    Ok(mut contrib) => out.append(&mut contrib),
                    Err(e) => return Err(e),
                }
            }
            Ok(out)
        });
        for part in parts {
            for (t, c) in part? {
                let dst = &mut coeffs[t as usize * dim..(t as usize + 1) * dim];
                for (d, s) in dst.iter_mut().zip(&c) {
                    *d += s;
                }
            }
        }
        first = last;
    }
    Ok(RtFlux { k, coeffs })
}

struct PatchCtx<'a> {
    mesh: &'a TetMesh,
    topo: &'a FaceTopology,
    eps: Permittivity<'a>,
    primal: PrimalPotential<'a>,
    tet_edges: Option<&'a [[(u32, i8); 6]]>,
    space: &'a RtSpace,
}

/// How one local nodal DOF of a patch tetrahedron enters the patch problem.
#[derive(Clone, Copy)]
enum Slot {
    /// Fixed to zero (face opposite the patch vertex, or natural boundary).
    Zero,
    /// Patch unknown index.
    Id(usize),
}

impl PatchCtx<'_> {
    /// `∇φ_h` on tetrahedron `t` at barycentric point `lam`.
    fn grad_phi(&self, t: usize, lam: &[f64; 4], grads: &[[f64; 3]; 4]) -> [f64; 3] {
        let tet = self.mesh.tets[t];
        let mut g = [0.0; 3];
        match self.primal {
            PrimalPotential::P1(phi) => {
                for a in 0..4 {
                    let v = phi[tet[a] as usize];
                    for d in 0..3 {
                        g[d] += v * grads[a][d];
                    }
                }
            }
            PrimalPotential::P2(phi) => {
                let basis = crate::elements::p2::tet_p2_grads(lam, grads);
                let te = &self.tet_edges.expect("P2 has an edge table")[t];
                let n = self.mesh.n_nodes();
                for (l, b) in basis.iter().enumerate() {
                    let v = if l < 4 {
                        phi[tet[l] as usize]
                    } else {
                        phi[n + te[l - 4].0 as usize]
                    };
                    for d in 0..3 {
                        g[d] += v * b[d];
                    }
                }
            }
        }
        g
    }

    /// Solve the patch problem of vertex `a`; returns per-tetrahedron
    /// spanning-basis coefficients of `σ_a`.
    fn solve_patch(
        &self,
        a: u32,
        tets: &[u32],
    ) -> Result<Vec<(u32, Vec<f64>)>, ElectrostaticError> {
        let sp = self.space;
        let dim = sp.dim;
        let nfm = sp.face_mono.len();
        let n_div = sp.vec_mono.len();
        let np = sp.tet_q.len();
        // Patch unknown numbering.
        let mut shared: Vec<(u32, usize)> = Vec::new();
        let mut n_ids = 0usize;
        let mut has_crack = false;
        let mut slots: Vec<Vec<Slot>> = Vec::with_capacity(tets.len());
        for &t in tets {
            let t = t as usize;
            let la = self.mesh.tets[t].iter().position(|&v| v == a).unwrap();
            let mut s = vec![Slot::Zero; dim];
            for f in 0..4 {
                if f == la {
                    continue;
                }
                let fi = self.topo.tet_faces[t][f];
                let base = if self.topo.face_conductor[fi as usize] != NONE {
                    has_crack = true;
                    let b = n_ids;
                    n_ids += nfm;
                    Some(b)
                } else if self.topo.face_tets[fi as usize][1] == NONE {
                    None
                } else if let Some(&(_, b)) = shared.iter().find(|(g, _)| *g == fi) {
                    Some(b)
                } else {
                    let b = n_ids;
                    n_ids += nfm;
                    shared.push((fi, b));
                    Some(b)
                };
                if let Some(b) = base {
                    for i in 0..nfm {
                        s[f * nfm + i] = Slot::Id(b + i);
                    }
                }
            }
            slots.push(s);
        }
        // Patch unknowns are the face DOFs only: each tetrahedron's interior
        // (bubble) DOFs and its non-constant divergence multipliers are
        // eliminated locally (static condensation, below).
        let n = n_ids;
        let mut s = vec![[0.0; 3]; dim];
        let mut dv = vec![0.0; dim];
        let mut qv = vec![0.0; n_div];
        let mut phi = Mat::<f64>::zeros(3 * np, dim);
        let mut ms = Mat::<f64>::zeros(dim, dim);
        let mut msb = Mat::<f64>::zeros(dim, dim);
        let mut locals: Vec<TetLocal> = Vec::with_capacity(tets.len());
        for (pt, &t) in tets.iter().enumerate() {
            let t = t as usize;
            let geo = TetGeom::new(self.mesh, t);
            let la = self.mesh.tets[t].iter().position(|&v| v == a).unwrap();
            let bvec = sp.nodal_basis(self.mesh, t, &geo);
            let bmat = Mat::<f64>::from_fn(dim, dim, |i, j| bvec[i * dim + j]);
            // ε⁻¹ = C Cᵀ, so s·ε⁻¹s' = (Cᵀs)·(Cᵀs').
            let ct = chol3_transpose(&self.eps.inverse(t));
            let e_mat = self.eps.matrix(t);
            let mut rs = vec![0.0; dim];
            let mut ds = vec![0.0; n_div * dim];
            let mut gs = vec![0.0; n_div];
            let mut qm = vec![0.0; n_div];
            for (qi, (lam, w)) in sp.tet_q.iter().enumerate() {
                let x = geo.point(lam);
                let xi = geo.xi(&x);
                sp.eval(&xi, &mut s);
                sp.div(&xi, geo.h, &mut dv);
                sp.scalar_monomials(&xi, &mut qv);
                let grad = self.grad_phi(t, lam, &geo.grads);
                let psi = lam[la];
                // ∇ψ_a·σ_h = −∇λ_a·ε∇φ_h.
                let div_data = -dot(geo.grads[la], matvec(&e_mat, grad));
                let sw = w.sqrt();
                for j in 0..dim {
                    let v = matvec(&ct, s[j]);
                    for d in 0..3 {
                        phi[(3 * qi + d, j)] = sw * v[d];
                    }
                    rs[j] -= w * psi * dot(grad, s[j]);
                }
                for l in 0..n_div {
                    let wq = w * qv[l];
                    for j in 0..dim {
                        ds[l * dim + j] += wq * dv[j];
                    }
                    gs[l] += wq * div_data;
                    qm[l] += wq;
                }
            }
            // Mn = |T| Bᵀ (ΦᵀΦ) B; rn = |T| Bᵀ rs; Dn = Ds B (rows are means).
            let mut mn = Mat::<f64>::zeros(dim, dim);
            matmul(
                ms.as_mut(),
                Accum::Replace,
                phi.transpose(),
                phi.as_ref(),
                1.0,
                Par::Seq,
            );
            matmul(
                msb.as_mut(),
                Accum::Replace,
                ms.as_ref(),
                bmat.as_ref(),
                1.0,
                Par::Seq,
            );
            matmul(
                mn.as_mut(),
                Accum::Replace,
                bmat.transpose(),
                msb.as_ref(),
                geo.vol,
                Par::Seq,
            );
            let rn: Vec<f64> = matvec_tn(&bvec, &rs, dim, dim)
                .iter()
                .map(|v| v * geo.vol)
                .collect();
            let dn = matmul_rm(&ds, &bvec, n_div, dim, dim);
            locals.push(TetLocal {
                face: (0..4 * nfm)
                    .filter_map(|l| match slots[pt][l] {
                        Slot::Id(i) => Some((l, i)),
                        Slot::Zero => None,
                    })
                    .collect(),
                mn,
                rn,
                dn,
                g: gs,
                q_mean: qm,
                vol: geo.vol,
                basis: bvec,
                back: None,
            });
        }
        // Compatibility of a patch without conductor faces: its data must
        // integrate to zero (Galerkin orthogonality, up to the solver
        // tolerance); remove the mean. The constant monomial is row 0.
        if !has_crack {
            let total: f64 = locals.iter().map(|l| l.vol * l.g[0]).sum();
            let shift = total / locals.iter().map(|l| l.vol).sum::<f64>();
            for l in &mut locals {
                for (g, q) in l.g.iter_mut().zip(&l.q_mean) {
                    *g -= shift * q;
                }
            }
        }
        // Condense every tetrahedron onto its face DOFs and assemble.
        let n_t = tets.len();
        let mut m = Mat::<f64>::zeros(n, n);
        let mut r = Mat::<f64>::zeros(n, 1);
        let mut ct = Mat::<f64>::zeros(n, n_t);
        let mut gc = vec![0.0; n_t];
        for (pt, l) in locals.iter_mut().enumerate() {
            let cond = condense(l, 4 * nfm, dim, n_div).ok_or_else(|| {
                ElectrostaticError::ShapeMismatch(format!(
                    "vertex patch {a}: local RT problem of tet {} is singular",
                    tets[pt]
                ))
            })?;
            for (fi, &(_, pi)) in l.face.iter().enumerate() {
                r[(pi, 0)] += cond.rhs[fi];
                ct[(pi, pt)] += l.dn[l.face[fi].0];
                for (fj, &(_, pj)) in l.face.iter().enumerate() {
                    m[(pi, pj)] += cond.s[(fi, fj)];
                }
            }
            gc[pt] = l.g[0];
            l.back = Some(cond.back);
        }
        let mut keep = vec![true; n_t];
        if !has_crack {
            keep[0] = false; // implied by the other rows and the zero-flux boundary
        }
        let x = solve_kkt(m, r, &ct, &gc, &keep).ok_or_else(|| {
            ElectrostaticError::ShapeMismatch(format!(
                "vertex patch {a}: condensed RT patch matrix is not positive definite"
            ))
        })?;
        let mut out = Vec::with_capacity(tets.len());
        for (pt, &t) in tets.iter().enumerate() {
            let l = &locals[pt];
            let mut xl = vec![0.0; dim];
            for &(li, pi) in &l.face {
                xl[li] = x[pi];
            }
            let (xb, xh) = l.back.as_ref().unwrap();
            for (ii, xi) in xl[4 * nfm..].iter_mut().enumerate() {
                let mut v = xh[ii];
                for (fi, &(_, pi)) in l.face.iter().enumerate() {
                    v -= xb[(ii, fi)] * x[pi];
                }
                *xi = v;
            }
            let c = matvec_n(&l.basis, &xl, dim, dim);
            out.push((t, c));
        }
        Ok(out)
    }
}

/// Per-tetrahedron data of a patch problem, in the nodal basis.
struct TetLocal {
    /// `(local nodal index, patch id)` of the face DOFs that are unknowns.
    face: Vec<(usize, usize)>,
    /// `|T| Bᵀ M_s B`.
    mn: Mat<f64>,
    /// Linear term.
    rn: Vec<f64>,
    /// Divergence moments (row-major `n_div × dim`, means over `T`).
    dn: Vec<f64>,
    /// Divergence data moments.
    g: Vec<f64>,
    /// Means of the test monomials.
    q_mean: Vec<f64>,
    vol: f64,
    /// Nodal basis in spanning coefficients (row-major).
    basis: Vec<f64>,
    /// Back-substitution `x_I = x_h − X_B x_F`, filled by [`condense`].
    back: Option<(Mat<f64>, Vec<f64>)>,
}

/// A tetrahedron condensed onto its face DOFs.
struct Condensed {
    s: Mat<f64>,
    rhs: Vec<f64>,
    back: (Mat<f64>, Vec<f64>),
}

/// Eliminate the interior DOFs `I` and the non-constant divergence
/// multipliers of one tetrahedron. With `K = [[M_II, D_Iᵀ], [D_I, 0]]` (the
/// non-constant rows; the bubbles' divergence has zero mean, so the constant
/// row involves face DOFs only), `B = [M_IF; D_F]` and `h = [r_I; g]`, the
/// face block becomes `M_FF − Bᵀ K⁻¹ B` with right-hand side
/// `r_F − Bᵀ K⁻¹ h`, and `x_I` is the first block of `K⁻¹ (h − B x_F)`.
fn condense(l: &TetLocal, n_face_local: usize, dim: usize, n_div: usize) -> Option<Condensed> {
    let fidx: Vec<usize> = l.face.iter().map(|f| f.0).collect();
    let nf = fidx.len();
    let ni = dim - n_face_local;
    let no = n_div - 1;
    let iidx = |i: usize| n_face_local + i;
    let mut s = Mat::<f64>::from_fn(nf, nf, |i, j| l.mn[(fidx[i], fidx[j])]);
    let mut rhs: Vec<f64> = fidx.iter().map(|&i| l.rn[i]).collect();
    if ni == 0 {
        return Some(Condensed {
            s,
            rhs,
            back: (Mat::zeros(0, nf), Vec::new()),
        });
    }
    // M_II and its factor.
    let mut lii = Mat::<f64>::from_fn(ni, ni, |i, j| l.mn[(iidx(i), iidx(j))]);
    if !faer_cholesky(&mut lii) {
        return None;
    }
    let d_i = Mat::<f64>::from_fn(no, ni, |o, i| l.dn[(o + 1) * dim + iidx(i)]);
    // Schur complement of the local saddle: S_o = D_I M_II⁻¹ D_Iᵀ.
    let mut y = d_i.transpose().to_owned();
    faer::linalg::triangular_solve::solve_lower_triangular_in_place(
        lii.as_ref(),
        y.as_mut(),
        Par::Seq,
    );
    let mut so = Mat::<f64>::zeros(no, no);
    matmul(
        so.as_mut(),
        Accum::Replace,
        y.transpose(),
        y.as_ref(),
        1.0,
        Par::Seq,
    );
    if !faer_cholesky(&mut so) {
        return None;
    }
    // Right-hand sides [a; b] = [B | h]: columns 0..nf are B, column nf is h.
    let mut a = Mat::<f64>::from_fn(ni, nf + 1, |i, c| {
        if c < nf {
            l.mn[(iidx(i), fidx[c])]
        } else {
            l.rn[iidx(i)]
        }
    });
    let b = Mat::<f64>::from_fn(no, nf + 1, |o, c| {
        if c < nf {
            l.dn[(o + 1) * dim + fidx[c]]
        } else {
            l.g[o + 1]
        }
    });
    // x0 = M_II⁻¹ a; λ = S_o⁻¹ (D_I x0 − b); x = M_II⁻¹ (a − D_Iᵀ λ).
    let mut x0 = a.clone();
    faer_chol_solve(&lii, &mut x0);
    let mut lam = b.clone();
    matmul(
        lam.as_mut(),
        Accum::Replace,
        d_i.as_ref(),
        x0.as_ref(),
        1.0,
        Par::Seq,
    );
    for o in 0..no {
        for c in 0..=nf {
            lam[(o, c)] -= b[(o, c)];
        }
    }
    faer_chol_solve(&so, &mut lam);
    matmul(
        a.as_mut(),
        Accum::Add,
        d_i.transpose(),
        lam.as_ref(),
        -1.0,
        Par::Seq,
    );
    faer_chol_solve(&lii, &mut a);
    // Bᵀ K⁻¹ [B | h] = M_FI x + D_Fᵀ λ.
    for i in 0..nf {
        for c in 0..=nf {
            let mut v = 0.0;
            for ii in 0..ni {
                v += l.mn[(fidx[i], iidx(ii))] * a[(ii, c)];
            }
            for o in 0..no {
                v += l.dn[(o + 1) * dim + fidx[i]] * lam[(o, c)];
            }
            if c < nf {
                s[(i, c)] -= v;
            } else {
                rhs[i] -= v;
            }
        }
    }
    let xb = Mat::<f64>::from_fn(ni, nf, |ii, c| a[(ii, c)]);
    let xh: Vec<f64> = (0..ni).map(|ii| a[(ii, nf)]).collect();
    // Symmetrize the condensed block against roundoff.
    for i in 0..nf {
        for j in 0..i {
            let v = 0.5 * (s[(i, j)] + s[(j, i)]);
            s[(i, j)] = v;
            s[(j, i)] = v;
        }
    }
    Some(Condensed {
        s,
        rhs,
        back: (xb, xh),
    })
}

/// Sequential dense Cholesky `A = L Lᵀ` in place (lower triangle); `false`
/// if `A` is not numerically positive definite.
fn faer_cholesky(a: &mut Mat<f64>) -> bool {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::cholesky::llt::factor::{cholesky_in_place, cholesky_in_place_scratch};
    let n = a.nrows();
    let mut mem = MemBuffer::new(cholesky_in_place_scratch::<f64>(
        n,
        Par::Seq,
        Default::default(),
    ));
    let stack = MemStack::new(&mut mem);
    cholesky_in_place(
        a.as_mut(),
        Default::default(),
        Par::Seq,
        stack,
        Default::default(),
    )
    .is_ok()
}

/// Solve `L Lᵀ X = B` in place with the factor of [`faer_cholesky`].
fn faer_chol_solve(l: &Mat<f64>, b: &mut Mat<f64>) {
    use faer::linalg::triangular_solve::{
        solve_lower_triangular_in_place, solve_upper_triangular_in_place,
    };
    solve_lower_triangular_in_place(l.as_ref(), b.as_mut(), Par::Seq);
    solve_upper_triangular_in_place(l.transpose(), b.as_mut(), Par::Seq);
}

/// `min ½xᵀMx − rᵀx` subject to the kept constraints `Dᵀ[:, row]·x = g[row]`
/// (`dt` holds `Dᵀ`), by the Schur complement `S = D M⁻¹ Dᵀ`. Dependent
/// constraint rows (a vanishing pivot of `S`) are skipped; whatever residual
/// that leaves is measured by the certificate.
fn solve_kkt(
    mut m: Mat<f64>,
    r: Mat<f64>,
    dt: &Mat<f64>,
    g: &[f64],
    keep: &[bool],
) -> Option<Vec<f64>> {
    use faer::linalg::triangular_solve::solve_lower_triangular_in_place;
    let n = m.nrows();
    if !faer_cholesky(&mut m) {
        return None;
    }
    let rows: Vec<usize> = (0..keep.len()).filter(|&i| keep[i]).collect();
    let nr = rows.len();
    let dk = Mat::<f64>::from_fn(n, nr, |i, c| dt[(i, rows[c])]);
    // Y = L⁻¹ Dᵀ, S = YᵀY.
    let mut y = dk.clone();
    solve_lower_triangular_in_place(m.as_ref(), y.as_mut(), Par::Seq);
    let mut s = Mat::<f64>::zeros(nr, nr);
    matmul(
        s.as_mut(),
        Accum::Replace,
        y.transpose(),
        y.as_ref(),
        1.0,
        Par::Seq,
    );
    // x0 = M⁻¹ r; rhs = D x0 − g.
    let mut x0 = r.clone();
    faer_chol_solve(&m, &mut x0);
    let mut rhs = Mat::<f64>::zeros(nr, 1);
    matmul(
        rhs.as_mut(),
        Accum::Replace,
        dk.transpose(),
        x0.as_ref(),
        1.0,
        Par::Seq,
    );
    for (c, &row) in rows.iter().enumerate() {
        rhs[(c, 0)] -= g[row];
    }
    let mut sf = s.clone();
    if faer_cholesky(&mut sf) {
        faer_chol_solve(&sf, &mut rhs);
    } else {
        let mut sv: Vec<f64> = (0..nr * nr).map(|ij| s[(ij / nr, ij % nr)]).collect();
        let active = semidefinite_cholesky(&mut sv, nr);
        let mut b: Vec<f64> = (0..nr).map(|i| rhs[(i, 0)]).collect();
        semidefinite_solve(&sv, nr, &active, &mut b);
        for (i, v) in b.iter().enumerate() {
            rhs[(i, 0)] = *v;
        }
    }
    // x = M⁻¹ (r − Dᵀ λ).
    let mut x = r;
    matmul(
        x.as_mut(),
        Accum::Add,
        dk.as_ref(),
        rhs.as_ref(),
        -1.0,
        Par::Seq,
    );
    faer_chol_solve(&m, &mut x);
    Some((0..n).map(|i| x[(i, 0)]).collect())
}

/// `Cᵀ` for the lower Cholesky factor `C` of an SPD 3×3 matrix.
fn chol3_transpose(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut c = [[0.0; 3]; 3];
    for j in 0..3 {
        let mut s = a[j][j];
        for k in 0..j {
            s -= c[j][k] * c[j][k];
        }
        c[j][j] = s.sqrt();
        for i in j + 1..3 {
            let mut v = a[i][j];
            for k in 0..j {
                v -= c[i][k] * c[j][k];
            }
            c[i][j] = v / c[j][j];
        }
    }
    std::array::from_fn(|i| std::array::from_fn(|j| c[j][i]))
}

// ─────────────────────────────────────────────────────────────────────────
// The local RT_k space and quadrature
// ─────────────────────────────────────────────────────────────────────────

/// Spanning basis, degrees of freedom and quadrature of `RT_k` on a
/// tetrahedron.
///
/// Spanning set in `ξ`: `m(ξ) e_d` for every monomial of degree `≤ k` and
/// direction `d`, then `ξ m(ξ)` for every monomial of degree exactly `k`.
/// Degrees of freedom (unisolvent): on each face `F`, the moments
/// `|F|⁻¹ ∫_F (σ·n_F) μ` against the degree-`k` monomials `μ` in the face
/// barycentrics of its globally sorted vertices, with `n_F` the unit normal
/// of `(x_q − x_p) × (x_r − x_p)` for sorted `p < q < r` (so two
/// tetrahedra see the same functionals); in the interior, the means of
/// `σ·(m e_d)` for monomials of degree `≤ k − 1`.
struct RtSpace {
    k: usize,
    vec_mono: Vec<[u32; 3]>,
    hom_mono: Vec<[u32; 3]>,
    face_mono: Vec<[u32; 3]>,
    int_mono: Vec<[u32; 3]>,
    dim: usize,
    /// Barycentric point and weight (weights sum to 1).
    tet_q: Vec<([f64; 4], f64)>,
    /// Barycentric point (sorted face vertices) and weight (sum 1).
    tri_q: Vec<([f64; 3], f64)>,
}

fn monomials(max_deg: usize, exact: bool) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    for deg in 0..=max_deg {
        if exact && deg != max_deg {
            continue;
        }
        for a in (0..=deg).rev() {
            for b in (0..=deg - a).rev() {
                out.push([a as u32, b as u32, (deg - a - b) as u32]);
            }
        }
    }
    out
}

impl RtSpace {
    fn new(k: usize) -> Self {
        let vec_mono = monomials(k, false);
        let hom_mono = monomials(k, true);
        let face_mono = monomials(k, true);
        let int_mono = if k == 0 {
            Vec::new()
        } else {
            monomials(k - 1, false)
        };
        let dim = 3 * vec_mono.len() + hom_mono.len();
        debug_assert_eq!(dim, RtFlux::dim(k));
        debug_assert_eq!(dim, 4 * face_mono.len() + 3 * int_mono.len());
        // Exact for degree 2k+2 (the energy) and k+p+1 ≤ 2k+2 (P2 data).
        let tet_q = duffy_tet(k + 3);
        let tri_q = duffy_tri(k + 2);
        Self {
            k,
            vec_mono,
            hom_mono,
            face_mono,
            int_mono,
            dim,
            tet_q,
            tri_q,
        }
    }

    /// `ξ_d^e` for `e ≤ MAX_K + 1`.
    fn powers(&self, xi: &[f64; 3]) -> [[f64; MAX_K + 2]; 3] {
        let mut p = [[1.0; MAX_K + 2]; 3];
        for d in 0..3 {
            for e in 1..MAX_K + 2 {
                p[d][e] = p[d][e - 1] * xi[d];
            }
        }
        p
    }

    fn mono(p: &[[f64; MAX_K + 2]; 3], e: &[u32; 3]) -> f64 {
        p[0][e[0] as usize] * p[1][e[1] as usize] * p[2][e[2] as usize]
    }

    fn eval(&self, xi: &[f64; 3], out: &mut [[f64; 3]]) {
        let p = self.powers(xi);
        let mut j = 0;
        for e in &self.vec_mono {
            let v = Self::mono(&p, e);
            for d in 0..3 {
                let mut s = [0.0; 3];
                s[d] = v;
                out[j] = s;
                j += 1;
            }
        }
        for e in &self.hom_mono {
            let v = Self::mono(&p, e);
            out[j] = [xi[0] * v, xi[1] * v, xi[2] * v];
            j += 1;
        }
    }

    /// Divergence with respect to `x` (`ξ = (x − x_T)/h`).
    fn div(&self, xi: &[f64; 3], h: f64, out: &mut [f64]) {
        let p = self.powers(xi);
        let mut j = 0;
        for e in &self.vec_mono {
            for d in 0..3 {
                out[j] = if e[d] == 0 {
                    0.0
                } else {
                    let mut ed = *e;
                    ed[d] -= 1;
                    e[d] as f64 * Self::mono(&p, &ed) / h
                };
                j += 1;
            }
        }
        for e in &self.hom_mono {
            out[j] = (3 + self.k) as f64 * Self::mono(&p, e) / h;
            j += 1;
        }
    }

    /// The scalar monomials of degree `≤ k` (the divergence test space).
    fn scalar_monomials(&self, xi: &[f64; 3], out: &mut [f64]) {
        let p = self.powers(xi);
        for (o, e) in out.iter_mut().zip(&self.vec_mono) {
            *o = Self::mono(&p, e);
        }
    }

    /// Columns: nodal basis functions (dual to the DOFs, local faces first,
    /// then interior) as spanning-basis coefficients. Row-major `dim × dim`.
    fn nodal_basis(&self, mesh: &TetMesh, t: usize, geo: &TetGeom) -> Vec<f64> {
        let dim = self.dim;
        let tet = mesh.tets[t];
        let mut a = vec![0.0; dim * dim];
        let mut s = vec![[0.0; 3]; dim];
        let nfm = self.face_mono.len();
        for (f, lf) in LOCAL_FACES.iter().enumerate() {
            let fv = sorted3(lf.map(|l| tet[l]));
            let p = fv.map(|v| mesh.nodes[v as usize]);
            let (n, _) = unit_normal(&p);
            for (lam, w) in &self.tri_q {
                let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                self.eval(&geo.xi(&x), &mut s);
                for (i, e) in self.face_mono.iter().enumerate() {
                    let mu = lam[0].powi(e[0] as i32)
                        * lam[1].powi(e[1] as i32)
                        * lam[2].powi(e[2] as i32);
                    let row = f * nfm + i;
                    for j in 0..dim {
                        a[row * dim + j] += w * mu * dot(s[j], n);
                    }
                }
            }
        }
        if !self.int_mono.is_empty() {
            let mut qv = vec![0.0; self.int_mono.len()];
            for (lam, w) in &self.tet_q {
                let xi = geo.xi(&geo.point(lam));
                self.eval(&xi, &mut s);
                let p = self.powers(&xi);
                for (o, e) in qv.iter_mut().zip(&self.int_mono) {
                    *o = Self::mono(&p, e);
                }
                for (i, q) in qv.iter().enumerate() {
                    for d in 0..3 {
                        let row = 4 * nfm + 3 * i + d;
                        for j in 0..dim {
                            a[row * dim + j] += w * q * s[j][d];
                        }
                    }
                }
            }
        }
        invert(&a, dim).expect("RT_k degrees of freedom are unisolvent on a non-degenerate tet")
    }
}

/// Gauss–Legendre nodes and weights on `[0, 1]`.
fn gauss_legendre01(n: usize) -> Vec<(f64, f64)> {
    let legendre = |x: f64| -> (f64, f64) {
        let (mut p0, mut p1) = (1.0, x);
        for k in 2..=n {
            let p2 = ((2 * k - 1) as f64 * x * p1 - (k - 1) as f64 * p0) / k as f64;
            p0 = p1;
            p1 = p2;
        }
        let dp = n as f64 * (x * p1 - p0) / (x * x - 1.0);
        (p1, dp)
    };
    (0..n)
        .map(|i| {
            let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
            for _ in 0..100 {
                let (p, dp) = legendre(x);
                let dx = p / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            let (_, dp) = legendre(x);
            let w = 2.0 / ((1.0 - x * x) * dp * dp);
            (0.5 * (x + 1.0), 0.5 * w)
        })
        .collect()
}

/// Collapsed-coordinate (Duffy) rule on the tetrahedron with `n³` points,
/// exact for total degree `2n − 3`; barycentric points, weights sum to 1.
fn duffy_tet(n: usize) -> Vec<([f64; 4], f64)> {
    let g = gauss_legendre01(n);
    let mut out = Vec::with_capacity(n * n * n);
    for &(u, wu) in &g {
        for &(v, wv) in &g {
            for &(w, ww) in &g {
                let x = u;
                let y = v * (1.0 - u);
                let z = w * (1.0 - u) * (1.0 - v);
                let weight = 6.0 * wu * wv * ww * (1.0 - u) * (1.0 - u) * (1.0 - v);
                out.push(([1.0 - x - y - z, x, y, z], weight));
            }
        }
    }
    out
}

/// Collapsed-coordinate rule on the triangle with `n²` points, exact for
/// total degree `2n − 2`; barycentric points, weights sum to 1.
fn duffy_tri(n: usize) -> Vec<([f64; 3], f64)> {
    let g = gauss_legendre01(n);
    let mut out = Vec::with_capacity(n * n);
    for &(u, wu) in &g {
        for &(v, wv) in &g {
            let x = u;
            let y = v * (1.0 - u);
            out.push(([1.0 - x - y, x, y], 2.0 * wu * wv * (1.0 - u)));
        }
    }
    out
}

/// Affine tetrahedron geometry.
struct TetGeom {
    v: [[f64; 3]; 4],
    c: [f64; 3],
    h: f64,
    vol: f64,
    grads: [[f64; 3]; 4],
}

impl TetGeom {
    fn new(mesh: &TetMesh, t: usize) -> Self {
        let v = mesh.tets[t].map(|i| mesh.nodes[i as usize]);
        let c = std::array::from_fn(|d| 0.25 * (v[0][d] + v[1][d] + v[2][d] + v[3][d]));
        let mut h: f64 = 0.0;
        for i in 0..4 {
            for j in i + 1..4 {
                h = h.max(norm(sub(v[i], v[j])));
            }
        }
        Self {
            v,
            c,
            h,
            vol: tet_volume(&v),
            grads: tet_bary_grads(&v),
        }
    }

    fn point(&self, lam: &[f64; 4]) -> [f64; 3] {
        std::array::from_fn(|d| (0..4).map(|i| lam[i] * self.v[i][d]).sum())
    }

    fn xi(&self, x: &[f64; 3]) -> [f64; 3] {
        std::array::from_fn(|d| (x[d] - self.c[d]) / self.h)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Small dense helpers
// ─────────────────────────────────────────────────────────────────────────

fn par_chunks<T: Send>(
    n: usize,
    chunk: usize,
    f: impl Fn(std::ops::Range<usize>) -> T + Sync,
) -> Vec<T> {
    let ranges: Vec<std::ops::Range<usize>> = (0..n)
        .step_by(chunk.max(1))
        .map(|s| s..(s + chunk).min(n))
        .collect();
    #[cfg(feature = "faer-parallel")]
    {
        use rayon::prelude::*;
        let threads = crate::eigen::parallel::resolve_num_threads();
        crate::eigen::parallel::install_on_pool(threads, || {
            ranges.into_par_iter().map(&f).collect()
        })
    }
    #[cfg(not(feature = "faer-parallel"))]
    {
        ranges.into_iter().map(f).collect()
    }
}

fn par_sort<T: Ord + Send>(v: &mut [T]) {
    #[cfg(feature = "faer-parallel")]
    {
        use rayon::prelude::*;
        v.par_sort_unstable();
    }
    #[cfg(not(feature = "faer-parallel"))]
    v.sort_unstable();
}

fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}

fn diag(e: f64) -> [[f64; 3]; 3] {
    [[e, 0.0, 0.0], [0.0, e, 0.0], [0.0, 0.0, e]]
}

fn det3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

fn inv3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let d = det3(m);
    let c = |i: usize, j: usize| {
        let (r0, r1) = ((i + 1) % 3, (i + 2) % 3);
        let (c0, c1) = ((j + 1) % 3, (j + 2) % 3);
        m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
    };
    std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / d))
}

fn matvec(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `a + s1 b + s2 c`.
fn add3(a: [f64; 3], b: [f64; 3], s1: f64, c: [f64; 3], s2: f64) -> [f64; 3] {
    std::array::from_fn(|d| a[d] + s1 * b[d] + s2 * c[d])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Unit normal of `(p1 − p0) × (p2 − p0)` and the triangle area.
fn unit_normal(p: &[[f64; 3]; 3]) -> ([f64; 3], f64) {
    let c = cross(sub(p[1], p[0]), sub(p[2], p[0]));
    let l = norm(c);
    (scale3(c, 1.0 / l), 0.5 * l)
}

fn tet_volume(v: &[[f64; 3]; 4]) -> f64 {
    let (a, b, c) = (sub(v[1], v[0]), sub(v[2], v[0]), sub(v[3], v[0]));
    dot(a, cross(b, c)).abs() / 6.0
}

fn combine(c: &[f64], s: &[[f64; 3]]) -> [f64; 3] {
    let mut v = [0.0; 3];
    for (ci, si) in c.iter().zip(s) {
        for d in 0..3 {
            v[d] += ci * si[d];
        }
    }
    v
}

/// Row-major `(m×k) · (k×n)`.
fn matmul_rm(a: &[f64], b: &[f64], m: usize, k: usize, n: usize) -> Vec<f64> {
    let mut c = vec![0.0; m * n];
    for i in 0..m {
        for l in 0..k {
            let ail = a[i * k + l];
            if ail != 0.0 {
                for j in 0..n {
                    c[i * n + j] += ail * b[l * n + j];
                }
            }
        }
    }
    c
}

/// `Aᵀ x` for row-major `A` (`k×m`).
fn matvec_tn(a: &[f64], x: &[f64], k: usize, m: usize) -> Vec<f64> {
    let mut y = vec![0.0; m];
    for l in 0..k {
        for i in 0..m {
            y[i] += a[l * m + i] * x[l];
        }
    }
    y
}

/// `A x` for row-major `A` (`m×n`).
fn matvec_n(a: &[f64], x: &[f64], m: usize, n: usize) -> Vec<f64> {
    (0..m)
        .map(|i| (0..n).map(|j| a[i * n + j] * x[j]).sum())
        .collect()
}

/// Gauss–Jordan inverse with partial pivoting (row-major).
fn invert(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut m = a.to_vec();
    let mut inv = vec![0.0; n * n];
    for i in 0..n {
        inv[i * n + i] = 1.0;
    }
    for col in 0..n {
        let piv =
            (col..n).max_by(|&x, &y| m[x * n + col].abs().total_cmp(&m[y * n + col].abs()))?;
        let pv = m[piv * n + col];
        if pv == 0.0 || !pv.is_finite() {
            return None;
        }
        if piv != col {
            for j in 0..n {
                m.swap(piv * n + j, col * n + j);
                inv.swap(piv * n + j, col * n + j);
            }
        }
        let ip = 1.0 / pv;
        for j in 0..n {
            m[col * n + j] *= ip;
            inv[col * n + j] *= ip;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = m[row * n + col];
            if f != 0.0 {
                for j in 0..n {
                    m[row * n + j] -= f * m[col * n + j];
                    inv[row * n + j] -= f * inv[col * n + j];
                }
            }
        }
    }
    Some(inv)
}

/// In-place lower Cholesky of a row-major SPD matrix; `false` if not SPD.
fn cholesky(a: &mut [f64], n: usize) -> bool {
    for j in 0..n {
        let mut s = a[j * n + j];
        for k in 0..j {
            s -= a[j * n + k] * a[j * n + k];
        }
        if !s.is_finite() || s <= 0.0 {
            return false;
        }
        let l = s.sqrt();
        a[j * n + j] = l;
        for i in j + 1..n {
            let mut v = a[i * n + j];
            for k in 0..j {
                v -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = v / l;
        }
    }
    true
}

/// Solve `L y = b` in place (lower factor from [`cholesky`]).
fn forward(l: &[f64], n: usize, b: &mut [f64]) {
    for i in 0..n {
        let mut v = b[i];
        for k in 0..i {
            v -= l[i * n + k] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
}

/// Solve `L Lᵀ x = b` in place.
fn chol_solve(l: &[f64], n: usize, b: &mut [f64]) {
    forward(l, n, b);
    for i in (0..n).rev() {
        let mut v = b[i];
        for k in i + 1..n {
            v -= l[k * n + i] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
}

/// Cholesky of a positive-semidefinite matrix that skips pivots below
/// `1e-12 ×` the largest diagonal (dependent rows). Returns the active mask;
/// inactive rows and columns of the factor are zero.
fn semidefinite_cholesky(a: &mut [f64], n: usize) -> Vec<bool> {
    let scale = (0..n).map(|i| a[i * n + i]).fold(0.0_f64, f64::max);
    let tol = 1e-12 * scale;
    let mut active = vec![true; n];
    for j in 0..n {
        let mut s = a[j * n + j];
        for k in 0..j {
            s -= a[j * n + k] * a[j * n + k];
        }
        if s.is_nan() || s <= tol {
            active[j] = false;
            for i in j..n {
                a[i * n + j] = 0.0;
            }
            continue;
        }
        let l = s.sqrt();
        a[j * n + j] = l;
        for i in j + 1..n {
            let mut v = a[i * n + j];
            for k in 0..j {
                v -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = v / l;
        }
    }
    active
}

fn semidefinite_solve(l: &[f64], n: usize, active: &[bool], b: &mut [f64]) {
    for i in 0..n {
        if !active[i] {
            b[i] = 0.0;
            continue;
        }
        let mut v = b[i];
        for k in 0..i {
            v -= l[i * n + k] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
    for i in (0..n).rev() {
        if !active[i] {
            b[i] = 0.0;
            continue;
        }
        let mut v = b[i];
        for k in i + 1..n {
            v -= l[k * n + i] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::electrostatic::{
        Electrode, assemble_electrostatic, assemble_electrostatic_p2_sheets,
        extract_capacitance_amg_pcg_fields, extract_capacitance_p2_amg_pcg_fields,
    };
    use crate::mesh::cube_tet_mesh;
    use crate::mesh::red_refine::red_refine;

    fn tight() -> AmgPcgOptions {
        AmgPcgOptions {
            rel_tol: 1e-12,
            ..AmgPcgOptions::default()
        }
    }

    /// Faces whose three vertices satisfy `on` and whose centroid satisfies
    /// `inside`.
    fn faces_where(
        mesh: &TetMesh,
        on: impl Fn(&[f64; 3]) -> bool,
        inside: impl Fn(&[f64; 3]) -> bool,
    ) -> Vec<[u32; 3]> {
        let mut out = std::collections::BTreeSet::new();
        for tet in &mesh.tets {
            for lf in LOCAL_FACES {
                let f = sorted3(lf.map(|l| tet[l]));
                let p = f.map(|v| mesh.nodes[v as usize]);
                let c = std::array::from_fn(|d| (p[0][d] + p[1][d] + p[2][d]) / 3.0);
                if p.iter().all(&on) && inside(&c) {
                    out.insert(f);
                }
            }
        }
        out.into_iter().collect()
    }

    fn nodes_of(tris: &[[u32; 3]]) -> Vec<u32> {
        let mut v: Vec<u32> = tris.iter().flatten().copied().collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// A conductor set: driven first, then floating, then ground.
    struct Problem {
        mesh: TetMesh,
        eps: Vec<f64>,
        driven: Vec<[u32; 3]>,
        floating: Vec<Vec<[u32; 3]>>,
        ground: Vec<[u32; 3]>,
    }

    impl Problem {
        fn conductors(&self) -> Vec<DualConductor> {
            let mut c = vec![DualConductor {
                name: "driven".into(),
                role: ConductorRole::Driven,
                triangles: self.driven.clone(),
            }];
            for (i, f) in self.floating.iter().enumerate() {
                c.push(DualConductor {
                    name: format!("floating{i}"),
                    role: ConductorRole::Floating,
                    triangles: f.clone(),
                });
            }
            c.push(DualConductor {
                name: "ground".into(),
                role: ConductorRole::Ground,
                triangles: self.ground.clone(),
            });
            c
        }

        fn topology(&self) -> FaceTopology {
            FaceTopology::new(&self.mesh, &self.conductors()).unwrap()
        }

        /// Conforming primal `C` (F) and its floating-reduced potential.
        fn primal(&self, order: usize) -> (f64, Vec<f64>) {
            let mut electrodes = vec![Electrode {
                name: "driven".into(),
                nodes: nodes_of(&self.driven),
                voltage: 1.0,
            }];
            for f in &self.floating {
                electrodes.push(Electrode {
                    name: "floating".into(),
                    nodes: nodes_of(f),
                    voltage: 0.0,
                });
            }
            let ground = nodes_of(&self.ground);
            let rho = vec![0.0; self.mesh.n_tets()];
            let (cm, rep, phis) = if order == 1 {
                let sys = assemble_electrostatic(&self.mesh, &self.eps, &rho, &electrodes, &ground)
                    .unwrap();
                extract_capacitance_amg_pcg_fields(&sys, &electrodes, &ground, tight()).unwrap()
            } else {
                let mut tris: Vec<&[[u32; 3]]> = vec![&self.driven];
                tris.extend(self.floating.iter().map(Vec::as_slice));
                let (sys, dofs) = assemble_electrostatic_p2_sheets(
                    &self.mesh,
                    &self.eps,
                    &rho,
                    &electrodes,
                    &tris,
                    &ground,
                    &self.ground,
                )
                .unwrap();
                extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, tight()).unwrap()
            };
            assert!(rep.rel_residual.iter().all(|r| *r < 1e-10));
            let v = floating_potentials(&cm.c).unwrap();
            let mut phi = phis[0].clone();
            for (vf, pf) in v.iter().zip(&phis[1..]) {
                for (a, b) in phi.iter_mut().zip(pf) {
                    *a += vf * b;
                }
            }
            let c = cm.c[0][0]
                + v.iter()
                    .enumerate()
                    .map(|(j, x)| x * cm.c[0][j + 1])
                    .sum::<f64>();
            (c, phi)
        }

        fn equilibrated(&self, order: usize, k: usize) -> (f64, DualBound) {
            let topo = self.topology();
            let (c, phi) = self.primal(order);
            let primal = if order == 1 {
                PrimalPotential::P1(&phi)
            } else {
                PrimalPotential::P2(&phi)
            };
            let eps = Permittivity::Scalar(&self.eps);
            let flux = equilibrated_flux(&self.mesh, &topo, eps, primal, k).unwrap();
            (
                c,
                certify_lower_bound(&self.mesh, &topo, eps, &flux).unwrap(),
            )
        }

        fn crouzeix_raviart(&self) -> (CrouzeixRaviartReport, DualBound) {
            let topo = self.topology();
            let eps = Permittivity::Scalar(&self.eps);
            let (flux, rep) = crouzeix_raviart_flux(&self.mesh, &topo, eps, tight()).unwrap();
            let bound = certify_lower_bound(&self.mesh, &topo, eps, &flux).unwrap();
            (rep, bound)
        }

        fn red_refined(&self) -> Self {
            let r = red_refine(&self.mesh);
            Self {
                eps: r.lift_per_tet(&self.eps),
                driven: r.lift_triangles(&self.driven),
                floating: self.floating.iter().map(|f| r.lift_triangles(f)).collect(),
                ground: r.lift_triangles(&self.ground),
                mesh: r.mesh,
            }
        }
    }

    fn factorial(n: u32) -> f64 {
        (1..=n).map(f64::from).product()
    }

    #[test]
    fn duffy_rules_are_exact_to_their_degree() {
        for n in 2..6usize {
            let tet = duffy_tet(n);
            let tri = duffy_tri(n);
            for e in monomials(2 * n - 3, false) {
                // ∫ λ1^a λ2^b λ3^c over the unit tet, normalized by its volume.
                let exact = 6.0 * factorial(e[0]) * factorial(e[1]) * factorial(e[2])
                    / factorial(e[0] + e[1] + e[2] + 3);
                let q: f64 = tet
                    .iter()
                    .map(|(l, w)| {
                        w * l[1].powi(e[0] as i32) * l[2].powi(e[1] as i32) * l[3].powi(e[2] as i32)
                    })
                    .sum();
                assert!((q - exact).abs() < 1e-14, "tet n={n} {e:?}: {q} vs {exact}");
            }
            for deg in 0..=(2 * n - 2) as u32 {
                for a in 0..=deg {
                    let b = deg - a;
                    let exact = 2.0 * factorial(a) * factorial(b) / factorial(a + b + 2);
                    let q: f64 = tri
                        .iter()
                        .map(|(l, w)| w * l[1].powi(a as i32) * l[2].powi(b as i32))
                        .sum();
                    assert!((q - exact).abs() < 1e-14, "tri n={n} ({a},{b})");
                }
            }
        }
    }

    /// The nodal basis is dual to the degrees of freedom: a single-tet RT_k
    /// field with one unit face moment has that moment and no other.
    #[test]
    fn rt_nodal_basis_is_dual_to_the_face_moments() {
        let mesh = cube_tet_mesh(1, 1.0);
        for k in 0..=MAX_K {
            let sp = RtSpace::new(k);
            let geo = TetGeom::new(&mesh, 2);
            let b = sp.nodal_basis(&mesh, 2, &geo);
            let tet = mesh.tets[2];
            let nfm = sp.face_mono.len();
            let mut s = vec![[0.0; 3]; sp.dim];
            for col in 0..sp.dim {
                let c: Vec<f64> = (0..sp.dim).map(|j| b[j * sp.dim + col]).collect();
                for (f, lf) in LOCAL_FACES.iter().enumerate() {
                    let p = sorted3(lf.map(|l| tet[l])).map(|v| mesh.nodes[v as usize]);
                    let (n, _) = unit_normal(&p);
                    for (i, e) in sp.face_mono.iter().enumerate() {
                        let m: f64 = sp
                            .tri_q
                            .iter()
                            .map(|(lam, w)| {
                                let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                                sp.eval(&geo.xi(&x), &mut s);
                                let mu = lam[0].powi(e[0] as i32)
                                    * lam[1].powi(e[1] as i32)
                                    * lam[2].powi(e[2] as i32);
                                w * mu * dot(combine(&c, &s), n)
                            })
                            .sum();
                        let want = if f * nfm + i == col { 1.0 } else { 0.0 };
                        assert!(
                            (m - want).abs() < 1e-9, // Vandermonde conditioning grows with k
                            "k={k} col={col} face={f} i={i}: {m}"
                        );
                    }
                }
            }
        }
    }

    // ── Certificate terms against closed forms (#1062 review) ────────────
    //
    // Each test builds a flux by `RT_k` interpolation of a known field (exact
    // for a field in `RT_k`) and checks one term of the certificate against
    // its analytic value. Together they catch a face `L¹` term taken as
    // `|F|·|mean q|` or by one-point quadrature, a dropped divergence term,
    // and an energy evaluated with an inexact rule.

    /// A mesh from explicit nodes and tets.
    fn tet_mesh(nodes: Vec<[f64; 3]>, tets: Vec<[u32; 4]>) -> TetMesh {
        TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        }
    }

    /// The unit right tetrahedron, `x, y, z ≥ 0`, `x + y + z ≤ 1`.
    fn unit_tet() -> TetMesh {
        tet_mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 3]],
        )
    }

    /// Topology with one driven face and one ground face.
    fn two_face_topology(mesh: &TetMesh, driven: [u32; 3], ground: [u32; 3]) -> FaceTopology {
        FaceTopology::new(
            mesh,
            &[
                DualConductor {
                    name: "driven".into(),
                    role: ConductorRole::Driven,
                    triangles: vec![driven],
                },
                DualConductor {
                    name: "ground".into(),
                    role: ConductorRole::Ground,
                    triangles: vec![ground],
                },
            ],
        )
        .unwrap()
    }

    /// The `RT_k` interpolant of the per-tet field `f(t, x)`: its face and
    /// interior moments, evaluated with rules finer than the space's own,
    /// mapped through the nodal basis. Exact when `f(t, ·)` lies in `RT_k`.
    fn interpolate(mesh: &TetMesh, k: usize, f: impl Fn(usize, &[f64; 3]) -> [f64; 3]) -> RtFlux {
        let sp = RtSpace::new(k);
        let (dim, nfm) = (sp.dim, sp.face_mono.len());
        let tri_q = duffy_tri(MAX_K + 3);
        let tet_q = duffy_tet(MAX_K + 4);
        let mut coeffs = Vec::with_capacity(dim * mesh.n_tets());
        for t in 0..mesh.n_tets() {
            let geo = TetGeom::new(mesh, t);
            let b = sp.nodal_basis(mesh, t, &geo);
            let tet = mesh.tets[t];
            let mut dofs = vec![0.0; dim];
            for (fi, lf) in LOCAL_FACES.iter().enumerate() {
                let p = sorted3(lf.map(|l| tet[l])).map(|v| mesh.nodes[v as usize]);
                let (n, _) = unit_normal(&p);
                for (lam, w) in &tri_q {
                    let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                    let fn_x = dot(f(t, &x), n);
                    for (i, e) in sp.face_mono.iter().enumerate() {
                        let mu = lam[0].powi(e[0] as i32)
                            * lam[1].powi(e[1] as i32)
                            * lam[2].powi(e[2] as i32);
                        dofs[fi * nfm + i] += w * mu * fn_x;
                    }
                }
            }
            for (lam, w) in &tet_q {
                let x = geo.point(lam);
                let pw = sp.powers(&geo.xi(&x));
                let v = f(t, &x);
                for (i, e) in sp.int_mono.iter().enumerate() {
                    let q = RtSpace::mono(&pw, e);
                    for d in 0..3 {
                        dofs[4 * nfm + 3 * i + d] += w * q * v[d];
                    }
                }
            }
            for j in 0..dim {
                coeffs.push((0..dim).map(|r| b[j * dim + r] * dofs[r]).sum());
            }
        }
        RtFlux { k, coeffs }
    }

    /// A closed-form test field.
    type Field = fn(&[f64; 3]) -> [f64; 3];

    /// `N` is the driven charge minus every penalty (no floating conductor).
    fn assert_numerator_identity(b: &DualBound) {
        let want = b.charges[0] - (b.interior_jump_l1 + b.boundary_flux_l1 + b.divergence_l1);
        assert!((b.numerator - want).abs() < 1e-12, "{b:?}");
    }

    /// `|F|^½ ‖x − ⅓‖_{L²(F)}` on the unit right triangle in `z = 0` (area ½;
    /// there `x` has mean ⅓ and variance 1/18), that is `√(½ · ½/18) = 1/√72`.
    /// The function has zero mean but a nonzero `L¹` norm.
    fn zero_mean_face_penalty() -> f64 {
        1.0 / 72.0_f64.sqrt()
    }

    /// (a) An interior normal jump with zero mean and nonzero `L¹` norm. Two
    /// tets share the face `z = 0`; above it `D = (0, 0, x − ⅓)` (divergence
    /// free, no flux through the upper tet's natural faces), below it
    /// `D = 0`. The jump penalty must be the Cauchy–Schwarz value
    /// `|F|^½ ‖q‖_{L²(F)}` exactly: a signed mean gives 0, a one-point rule a
    /// different number.
    #[test]
    fn zero_mean_interior_jump_is_charged_its_l2_upper_bound() {
        let mesh = tet_mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
            ],
            vec![[0, 1, 2, 3], [0, 1, 2, 4]],
        );
        // Driven: the slanted face of the upper tet; ground: y = 0 below.
        let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 4]);
        for k in 1..=MAX_K {
            let flux = interpolate(&mesh, k, |t, x| {
                if t == 0 {
                    [0.0, 0.0, x[0] - 1.0 / 3.0]
                } else {
                    [0.0; 3]
                }
            });
            let b = certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&[1.0, 1.0]), &flux)
                .unwrap();
            assert!(
                (b.interior_jump_l1 - zero_mean_face_penalty()).abs() < 1e-9,
                "k={k}: jump penalty {} vs {}",
                b.interior_jump_l1,
                zero_mean_face_penalty()
            );
            assert!(
                b.boundary_flux_l1 < 1e-9 && b.divergence_l1 < 1e-9,
                "k={k}: {b:?}"
            );
            assert_numerator_identity(&b);
        }
    }

    /// (b) A non-solenoidal flux on one tet (driven slanted face, ground
    /// `z = 0`, natural faces `x = 0` and `y = 0` with zero normal flux).
    /// `D = (x, 0, 0)` has `div D = 1`, so the penalty is
    /// `|T|^½ ‖1‖_{L²(T)} = |T| = 1/6`; `D = (x², 0, 0)` has `div D = 2x`,
    /// penalty `√(|T| · 4∫x²) = √(1/6 · 1/15) = 1/√90`, above the true
    /// `∫|2x| = 1/12`. Dropping the term gives 0.
    #[test]
    fn divergence_penalty_matches_the_closed_form() {
        let mesh = unit_tet();
        let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 2]);
        let eps = [1.0];
        let cases: [(usize, f64, Field); 2] = [
            (1, 1.0 / 6.0, |x| [x[0], 0.0, 0.0]),
            (2, 1.0 / 90.0_f64.sqrt(), |x| [x[0] * x[0], 0.0, 0.0]),
        ];
        for (k_min, want, field) in cases {
            for k in k_min..=MAX_K {
                let flux = interpolate(&mesh, k, |_, x| field(x));
                let b =
                    certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&eps), &flux).unwrap();
                assert!(
                    (b.divergence_l1 - want).abs() < 1e-9,
                    "k={k}: divergence penalty {} vs {want}",
                    b.divergence_l1
                );
                assert!(b.boundary_flux_l1 < 1e-9 && b.interior_jump_l1 == 0.0);
                assert_numerator_identity(&b);
                // The divergence is paid for: N is below the driven charge.
                assert!(b.numerator <= b.charges[0] - want + 1e-9);
            }
        }
    }

    /// (c) Flux through the natural boundary of one tet (driven slanted face,
    /// ground `x = 0`, natural faces `y = 0` and `z = 0`). `D = (0, 0, x − ⅓)`
    /// is divergence free with zero-mean normal flux on `z = 0`, penalty
    /// `1/√72`; the constant `D = (0, 0, 1)` has `|D·n| = 1` there, penalty
    /// `|F| = ½` (Cauchy–Schwarz is an equality for a constant).
    #[test]
    fn boundary_flux_penalty_matches_the_closed_form() {
        let mesh = unit_tet();
        let topo = two_face_topology(&mesh, [1, 2, 3], [0, 2, 3]);
        let eps = [1.0];
        let cases: [(usize, f64, Field); 2] = [
            (1, zero_mean_face_penalty(), |x| {
                [0.0, 0.0, x[0] - 1.0 / 3.0]
            }),
            (0, 0.5, |_| [0.0, 0.0, 1.0]),
        ];
        for (k_min, want, field) in cases {
            for k in k_min..=MAX_K {
                let flux = interpolate(&mesh, k, |_, x| field(x));
                let b =
                    certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&eps), &flux).unwrap();
                assert!(
                    (b.boundary_flux_l1 - want).abs() < 1e-9,
                    "k={k}: boundary penalty {} vs {want}",
                    b.boundary_flux_l1
                );
                assert!(b.divergence_l1 < 1e-9 && b.interior_jump_l1 == 0.0);
                assert_numerator_identity(&b);
            }
        }
    }

    /// (d) `W_D = ∫ D·ε⁻¹D` of a degree-`k+1` field of `RT_k`,
    /// `D = (x, y, z) x^k`, on the tet `(0,0,0), (2,0,0), (0,1,0), (0,0,½)`,
    /// for every `k ≤ MAX_K`, against the closed form from
    /// `∫ u^a v^b w^c = a! b! c! / (a+b+c+3)!` on the unit tet (`x = 2u`,
    /// `y = v`, `z = w/2`, unit Jacobian). The integrand has degree `2k + 2`,
    /// so an inexact rule shows. Scalar `ε = 2.5` and the tensor
    /// `diag(2, 4, 8)`.
    #[test]
    fn single_tet_energy_matches_the_closed_form_for_every_order() {
        let mesh = tet_mesh(
            vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.5],
            ],
            vec![[0, 1, 2, 3]],
        );
        let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 2]);
        let tensor = [[[2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 8.0]]];
        for k in 0..=MAX_K {
            let kk = k as u32;
            let den = factorial(2 * kk + 5);
            let pow4 = 4f64.powi(k as i32);
            // ∫ x^{2k+2}, ∫ x^{2k} y², ∫ x^{2k} z² over the stretched tet.
            let ixx = pow4 * 4.0 * factorial(2 * kk + 2) / den;
            let iyy = pow4 * 2.0 * factorial(2 * kk) / den;
            let izz = pow4 * 0.25 * 2.0 * factorial(2 * kk) / den;
            let flux = interpolate(&mesh, k, |_, x| {
                let m = x[0].powi(k as i32);
                [x[0] * m, x[1] * m, x[2] * m]
            });
            let scalar =
                certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&[2.5]), &flux).unwrap();
            let want = (ixx + iyy + izz) / 2.5;
            assert!(
                (scalar.energy / want - 1.0).abs() < 1e-9,
                "k={k}: scalar W_D {} vs {want}",
                scalar.energy
            );
            let tens =
                certify_lower_bound(&mesh, &topo, Permittivity::Tensor(&tensor), &flux).unwrap();
            let want = ixx / 2.0 + iyy / 4.0 + izz / 8.0;
            assert!(
                (tens.energy / want - 1.0).abs() < 1e-9,
                "k={k}: tensor W_D {} vs {want}",
                tens.energy
            );
        }
    }

    /// A stacked two-dielectric parallel plate with a full-width floating
    /// sheet at the interface: the field is piecewise constant, so P1, P2,
    /// Crouzeix–Raviart and both equilibrations are exact, and the bracket
    /// collapses onto `ε₀ A / Σ(d_i/ε_i)`.
    #[test]
    fn parallel_plate_bracket_collapses_on_the_exact_value() {
        let pb = stacked_plate();
        let exact = EPS_0 / (0.5 / 4.0 + 0.5 / 1.0);
        for order in [1, 2] {
            let (c, b) = pb.equilibrated(order, 1);
            assert!(
                (c / exact - 1.0).abs() < 1e-9,
                "P{order} primal {c} vs {exact}"
            );
            assert!(
                (b.c_lower_f / exact - 1.0).abs() < 1e-8,
                "P{order} dual {}",
                b.c_lower_f
            );
            assert!(b.c_lower_f <= c * (1.0 + 1e-10));
            assert!(
                b.charges[1].abs() < 1e-9 * b.charges[0],
                "floating not neutral"
            );
        }
        let (rep, b) = pb.crouzeix_raviart();
        assert!((rep.c_cr_f / exact - 1.0).abs() < 1e-9);
        assert!((b.c_lower_f / exact - 1.0).abs() < 1e-8);
    }

    /// Structured annulus (one slab layer, natural top and bottom) between
    /// two similar regular `n`-gons with vertex radii `r_in < r_out`;
    /// returns the problem with the inner polygon driven and the outer one
    /// grounded.
    fn coax(n: usize, m: usize, r_in: f64, r_out: f64, thickness: f64) -> Problem {
        let idx = |i: usize, j: usize, l: usize| ((l * (m + 1) + j) * n + i % n) as u32;
        let mut nodes = Vec::new();
        for l in 0..2 {
            for j in 0..=m {
                let r = r_in * (r_out / r_in).powf(j as f64 / m as f64);
                for i in 0..n {
                    let th = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                    nodes.push([r * th.cos(), r * th.sin(), l as f64 * thickness]);
                }
            }
        }
        let mut tets = Vec::new();
        let perms = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        for i in 0..n {
            for j in 0..m {
                for p in perms {
                    let mut c = [i, j, 0];
                    let mut tet = [idx(c[0], c[1], c[2]); 4];
                    for (s, &axis) in p.iter().enumerate() {
                        c[axis] += 1;
                        tet[s + 1] = idx(c[0], c[1], c[2]);
                    }
                    tets.push(tet);
                }
            }
        }
        let mesh = TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        };
        let on_ring = |mesh: &TetMesh, j: usize| {
            let set: std::collections::BTreeSet<u32> = (0..2)
                .flat_map(|l| (0..n).map(move |i| idx(i, j, l)))
                .collect();
            let mut out = std::collections::BTreeSet::new();
            for tet in &mesh.tets {
                for lf in LOCAL_FACES {
                    let f = sorted3(lf.map(|x| tet[x]));
                    if f.iter().all(|v| set.contains(v)) {
                        out.insert(f);
                    }
                }
            }
            out.into_iter().collect::<Vec<_>>()
        };
        let driven = on_ring(&mesh, 0);
        let ground = on_ring(&mesh, m);
        let eps = vec![1.0; mesh.n_tets()];
        Problem {
            mesh,
            eps,
            driven,
            floating: Vec::new(),
            ground,
        }
    }

    /// The exact coaxial capacitance `2πε₀t/ln(b/a)` is bracketed with
    /// certainty by two polygonal families: an inner polygon around the
    /// circle `a` and an outer one inside `b` can only raise `C` (upper
    /// family), the reverse can only lower it (lower family). The P2 upper
    /// bound of the first and the equilibrated lower bound of the second
    /// must bracket the analytic value, and the bracket must shrink.
    #[test]
    fn coax_analytic_value_is_bracketed_and_the_bracket_converges() {
        let (a, b, t): (f64, f64, f64) = (1.0, 2.0, 0.25);
        let exact = 2.0 * std::f64::consts::PI * EPS_0 * t / (b / a).ln();
        let mut widths = Vec::new();
        for n in [12, 24, 48] {
            let m = n / 4;
            let cos = (std::f64::consts::PI / n as f64).cos();
            let upper = coax(n, m, a / cos, b, t);
            let lower = coax(n, m, a, b / cos, t);
            let (c_up, _) = upper.primal(2);
            let (c_lo_primal, bound) = lower.equilibrated(2, 1);
            let (cr, cr_bound) = lower.crouzeix_raviart();
            assert!(bound.c_lower_f <= c_lo_primal);
            assert!(cr_bound.c_lower_f <= c_lo_primal);
            assert!((cr_bound.c_lower_f / cr.c_cr_f - 1.0).abs() < 1e-8);
            assert!(
                bound.c_lower_f < exact && exact < c_up,
                "n={n}: [{}, {c_up}] misses {exact}",
                bound.c_lower_f
            );
            assert!(cr_bound.c_lower_f < exact);
            widths.push((c_up - bound.c_lower_f) / exact);
        }
        // Geometry and discretization error are both O(h^2) here: halving h
        // should divide the width by 4 (ratio 0.25). Measured: 0.236, 0.245.
        assert!(
            widths[1] < 0.27 * widths[0] && widths[2] < 0.27 * widths[1],
            "{widths:?}"
        );
    }

    /// A coplanar island and a floating feedline in a ground cutout on the
    /// interface of an `ε = 10` substrate (the #1057 judge's synthetic
    /// check, with a floating conductor). Sheet edges make the field
    /// singular, so nothing is exact; every certified lower bound must lie
    /// below every conforming upper bound across two levels and both
    /// orders.
    fn coplanar() -> Problem {
        let mesh = cube_tet_mesh(8, 1.0);
        let tol = 1e-12;
        let on = |p: &[f64; 3]| (p[2] - 0.5).abs() < tol;
        let inr = |c: &[f64; 3], x0: f64, x1: f64, y0: f64, y1: f64| {
            c[0] > x0 && c[0] < x1 && c[1] > y0 && c[1] < y1
        };
        let h = 0.125;
        let driven = faces_where(&mesh, on, |c| inr(c, 2.0 * h, 4.0 * h, 2.0 * h, 6.0 * h));
        let feed = faces_where(&mesh, on, |c| inr(c, 5.0 * h, 6.0 * h, 2.0 * h, 6.0 * h));
        let ground = faces_where(&mesh, on, |c| !inr(c, h, 7.0 * h, h, 7.0 * h));
        let (a, f, g) = (nodes_of(&driven), nodes_of(&feed), nodes_of(&ground));
        assert!(a.iter().all(|v| !f.contains(v) && !g.contains(v)));
        assert!(f.iter().all(|v| !g.contains(v)));
        let eps = mesh
            .tets
            .iter()
            .map(|t| {
                let z: f64 = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
                if z < 0.5 { 10.0 } else { 1.0 }
            })
            .collect();
        Problem {
            mesh,
            eps,
            driven,
            floating: vec![feed],
            ground,
        }
    }

    #[test]
    fn coplanar_island_lower_bounds_stay_below_every_upper_bound() {
        let coarse = coplanar();
        let fine = coarse.red_refined();
        let mut lowers = Vec::new();
        let mut uppers = Vec::new();
        let mut eq_p2 = Vec::new();
        for pb in [&coarse, &fine] {
            for order in [1, 2] {
                let (c, b) = pb.equilibrated(order, 1);
                // P1 on this coarse mesh is about 50 % off; the bound inherits it.
                assert!(
                    b.c_lower_f > 0.4 * c,
                    "P{order}: bound {} vs {c}",
                    b.c_lower_f
                );
                uppers.push(c);
                lowers.push(b.c_lower_f);
                if order == 2 {
                    eq_p2.push((b.c_lower_f, c));
                }
                // Floating neutrality survives the equilibration.
                assert!(b.charges[1].abs() < 1e-8 * b.charges[0]);
            }
            let (rep, b) = pb.crouzeix_raviart();
            assert!((b.c_lower_f / rep.c_cr_f - 1.0).abs() < 1e-8);
            lowers.push(b.c_lower_f);
        }
        let max_lower = lowers.iter().copied().fold(0.0, f64::max);
        let min_upper = uppers.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(max_lower < min_upper, "{lowers:?} vs {uppers:?}");
        // The P2-equilibrated bracket shrinks under refinement.
        let w = |(l, u): (f64, f64)| u - l;
        assert!(w(eq_p2[1]) < 0.75 * w(eq_p2[0]), "{eq_p2:?}");
    }

    /// The stacked parallel plate of
    /// [`parallel_plate_bracket_collapses_on_the_exact_value`]: exact for
    /// every discretization, so a violated bracket is unambiguous.
    fn stacked_plate() -> Problem {
        let mesh = cube_tet_mesh(4, 1.0);
        let tol = 1e-12;
        let plane = |z: f64| move |p: &[f64; 3]| (p[2] - z).abs() < tol;
        let all = |_: &[f64; 3]| true;
        let eps: Vec<f64> = mesh
            .tets
            .iter()
            .map(|t| {
                let z: f64 = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
                if z < 0.5 { 4.0 } else { 1.0 }
            })
            .collect();
        Problem {
            driven: faces_where(&mesh, plane(1.0), all),
            floating: vec![faces_where(&mesh, plane(0.5), all)],
            ground: faces_where(&mesh, plane(0.0), all),
            eps,
            mesh,
        }
    }

    /// Mutation: a flux that is admissible for the *grounded*-feedline
    /// problem carries a charge on the floating sheet. The certificate pays
    /// for it (here the bound drops to zero); with the `|Q_f|` term dropped
    /// it certifies the grounded `C_ii = 2ε₀` as a lower bound on the
    /// floating `C_Σ = 1.6ε₀`, above the exact (and conforming) value.
    #[test]
    fn mutation_ignoring_the_floating_charge_violates_the_bracket() {
        let pb = stacked_plate();
        let (c_floating, _) = pb.primal(2);
        let exact = EPS_0 / (0.5 / 4.0 + 0.5 / 1.0);
        assert!((c_floating / exact - 1.0).abs() < 1e-9);
        let grounded = Problem {
            mesh: pb.mesh.clone(),
            eps: pb.eps.clone(),
            driven: pb.driven.clone(),
            floating: Vec::new(),
            ground: pb.ground.iter().chain(&pb.floating[0]).copied().collect(),
        };
        let (c_grounded, phi) = grounded.primal(2);
        assert!((c_grounded / (2.0 * EPS_0) - 1.0).abs() < 1e-9);
        let topo = pb.topology();
        let eps = Permittivity::Scalar(&pb.eps);
        let flux = equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap();
        let honest = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::None).unwrap();
        let broken =
            certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::IgnoreFloatingCharge).unwrap();
        assert!(honest.c_lower_f <= c_floating);
        assert!((broken.c_lower_f / c_grounded - 1.0).abs() < 1e-8);
        assert!(
            broken.c_lower_f > 1.2 * c_floating,
            "{} vs {c_floating}",
            broken.c_lower_f
        );
    }

    /// Mutation: add to an admissible flux a constant field pointing away
    /// from the island in every substrate tetrahedron under it. That buys
    /// island charge without paying for it in energy at the island face, but
    /// the field leaves through the three other faces of each tetrahedron as
    /// normal jumps. The honest certificate charges those jumps; with the
    /// jump, boundary and divergence terms dropped, the "bound" approaches the
    /// thin-slab value `ε A² / Σ(|T|/ε)`, above the conforming upper bound.
    #[test]
    fn mutation_ignoring_flux_jumps_violates_the_bracket() {
        let pb = coplanar();
        let (c_p2, phi) = pb.primal(2);
        let topo = pb.topology();
        let eps = Permittivity::Scalar(&pb.eps);
        let mut flux =
            equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap();
        let dim = RtFlux::dim(1);
        for tri in &pb.driven {
            let f = topo.faces.binary_search(&sorted3(*tri)).unwrap();
            let p = tri.map(|v| pb.mesh.nodes[v as usize]);
            let (n, _) = unit_normal(&p);
            for &t in &topo.face_tets[f] {
                let t = t as usize;
                if pb.eps[t] < 2.0 {
                    continue; // substrate side only
                }
                let opp = pb.mesh.tets[t].iter().find(|v| !tri.contains(v)).unwrap();
                let side = dot(n, sub(pb.mesh.nodes[*opp as usize], p[0])).signum();
                // Constant monomial, directions 0..3, come first in RT_k.
                for d in 0..3 {
                    flux.coeffs[t * dim + d] += 1e3 * side * n[d];
                }
            }
        }
        let honest = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::None).unwrap();
        let broken = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::IgnoreSpurious).unwrap();
        assert!(honest.c_lower_f < c_p2);
        // The bought charge reappears, at least in full, as jump penalty.
        let clean = certify_lower_bound(
            &pb.mesh,
            &topo,
            eps,
            &equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap(),
        )
        .unwrap();
        let bought = honest.charges[0] - clean.charges[0];
        assert!(bought > 5.0 * clean.charges[0]);
        assert!(honest.interior_jump_l1 >= bought);
        assert!(
            broken.c_lower_f > 1.5 * c_p2,
            "{} vs {c_p2}",
            broken.c_lower_f
        );
    }

    /// A tensor `ε` that is the scalar one times the identity reproduces the
    /// scalar bound, and a genuinely anisotropic tensor still certifies a
    /// bound below the tensor-`ε` P1 conforming value.
    #[test]
    fn tensor_permittivity_matches_scalar_and_stays_below_the_tensor_primal() {
        use crate::assembly::electrostatic::assemble_electrostatic_tensor;
        let pb = coplanar();
        let topo = pb.topology();
        let tens: Vec<[[f64; 3]; 3]> = pb.eps.iter().map(|&e| diag(e)).collect();
        let (fs, _) =
            crouzeix_raviart_flux(&pb.mesh, &topo, Permittivity::Scalar(&pb.eps), tight()).unwrap();
        let (ft, _) =
            crouzeix_raviart_flux(&pb.mesh, &topo, Permittivity::Tensor(&tens), tight()).unwrap();
        let bs = certify_lower_bound(&pb.mesh, &topo, Permittivity::Scalar(&pb.eps), &fs).unwrap();
        let bt = certify_lower_bound(&pb.mesh, &topo, Permittivity::Tensor(&tens), &ft).unwrap();
        assert!((bs.c_lower_f / bt.c_lower_f - 1.0).abs() < 1e-9);
        // Anisotropic substrate (xy-coupled, SPD).
        let aniso: Vec<[[f64; 3]; 3]> = pb
            .eps
            .iter()
            .map(|&e| {
                if e > 1.0 {
                    [[9.3, 0.8, 0.0], [0.8, 9.7, 0.0], [0.0, 0.0, 11.5]]
                } else {
                    diag(1.0)
                }
            })
            .collect();
        let electrodes = vec![
            Electrode {
                name: "driven".into(),
                nodes: nodes_of(&pb.driven),
                voltage: 1.0,
            },
            Electrode {
                name: "feed".into(),
                nodes: nodes_of(&pb.floating[0]),
                voltage: 0.0,
            },
        ];
        let ground = nodes_of(&pb.ground);
        let rho = vec![0.0; pb.mesh.n_tets()];
        let sys =
            assemble_electrostatic_tensor(&pb.mesh, &aniso, &rho, &electrodes, &ground).unwrap();
        let (cm, _, phis) =
            extract_capacitance_amg_pcg_fields(&sys, &electrodes, &ground, tight()).unwrap();
        let v = floating_potentials(&cm.c).unwrap();
        let c = cm.c[0][0] + v[0] * cm.c[0][1];
        let phi: Vec<f64> = phis[0]
            .iter()
            .zip(&phis[1])
            .map(|(a, b)| a + v[0] * b)
            .collect();
        let eps = Permittivity::Tensor(&aniso);
        let flux = equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P1(&phi), 1).unwrap();
        let b = certify_lower_bound(&pb.mesh, &topo, eps, &flux).unwrap();
        let (fc, _) = crouzeix_raviart_flux(&pb.mesh, &topo, eps, tight()).unwrap();
        let bc = certify_lower_bound(&pb.mesh, &topo, eps, &fc).unwrap();
        assert!(b.c_lower_f < c && bc.c_lower_f < c);
        assert!(b.c_lower_f > 0.4 * c);
        assert!(b.charges[1].abs() < 1e-8 * b.charges[0]);
    }

    #[test]
    fn topology_rejects_bad_conductor_sets() {
        let mesh = cube_tet_mesh(2, 1.0);
        let tri = sorted3(LOCAL_FACES[0].map(|l| mesh.tets[0][l]));
        let c = |role, triangles: Vec<[u32; 3]>| DualConductor {
            name: "c".into(),
            role,
            triangles,
        };
        let only_driven = [c(ConductorRole::Driven, vec![tri])];
        assert!(FaceTopology::new(&mesh, &only_driven).is_err());
        let two_driven = [
            c(ConductorRole::Driven, vec![tri]),
            c(ConductorRole::Driven, vec![tri]),
            c(ConductorRole::Ground, vec![tri]),
        ];
        assert!(FaceTopology::new(&mesh, &two_driven).is_err());
        let shared = [
            c(ConductorRole::Driven, vec![tri]),
            c(ConductorRole::Ground, vec![tri]),
        ];
        assert!(FaceTopology::new(&mesh, &shared).is_err());
        let not_a_face = [
            c(ConductorRole::Driven, vec![[0, 1, 26]]),
            c(ConductorRole::Ground, vec![tri]),
        ];
        assert!(FaceTopology::new(&mesh, &not_a_face).is_err());
    }
}
