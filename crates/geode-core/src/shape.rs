//! Discrete-adjoint **shape / geometry** sensitivities:
//! `∂(scalar observable)/∂(geometry parameter)` **through** a linear FEM
//! solve (Epic #569, issue #571). The geometry counterpart of the
//! material-ε adjoint in [`crate::adjoint`].
//!
//! # Why this module exists
//!
//! [`crate::adjoint`] recovers `∂g/∂ε` — the sensitivity of a solved scalar
//! observable to a **material** parameter — from one forward + one adjoint
//! solve. Inverse design, however, mostly wants `∂g/∂(geometry)`: how the
//! figure-of-merit moves when a *dimension* of the device changes (a gap
//! width, a pad length, an electrode position). This module supplies that
//! **shape derivative** on the real, SPD scalar electrostatic operator
//! `−∇·(ε₀ ε_r ∇φ) = ρ` ([`crate::assembly::electrostatic`]).
//!
//! Shape derivatives are genuinely harder than material ones: the domain
//! itself moves, so the element stiffness depends on geometry through the
//! **element Jacobian** (edge vectors → barycentric gradients → volume),
//! not through a scalar prefactor. The dependence is nonlinear in the node
//! coordinates, so `∂K/∂(node)` is not simply "the assembly kernel applied
//! to a direction" (as it was for the linear-in-ε material case). We
//! differentiate the closed-form element kernel exactly (see below).
//!
//! # The adjoint identity for geometry
//!
//! Let the node coordinates be `X` and a geometry parameter be `θ`, with an
//! analytic **node-motion map** `θ ↦ X(θ)` on a **fixed mesh topology**.
//! The reduced electrostatic system is `K_ff(X) φ_free = b_free(X)` and the
//! observable is a smooth scalar `g(φ)` with **no** explicit geometry
//! dependence. Writing the full nodal potential `φ` (Dirichlet values in
//! place) and treating the free-row equilibrium `Σ_j K_full[i][j] φ_j = 0`
//! (`ρ = 0`, voltage-driven) as the residual, differentiating gives
//!
//! ```text
//!   K_ff (∂φ_free/∂X) = −[ (∂K_full/∂X) φ_full ]_free,
//! ```
//!
//! which already **absorbs** the geometry-dependence of the reduced RHS
//! `b_free = −K_fp φ_pinned` (the pinned potentials are `X`-independent
//! constants, so `∂b_free/∂X = −(∂K_fp/∂X) φ_pinned` is exactly the pinned
//! columns of `(∂K_full/∂X) φ_full`). Hence, with the **same** adjoint as
//! the material case `K_ffᵀ λ = ∂g/∂φ`,
//!
//! ```text
//!   ∂g/∂X_{n,d} = −λᵀ (∂K_full/∂X_{n,d}) φ
//!               = −Σ_{t} ε₀ ε_r[t] · λ_localᵀ (∂K_local(t)/∂X_{n,d}) φ_local,
//! ```
//!
//! a purely **local** contraction, one sweep over the tets, reusing the
//! single forward LU factorization for the adjoint (a transpose
//! back-substitution — never a refactorization). Chaining through the
//! node-motion Jacobian yields the design gradient
//!
//! ```text
//!   ∂g/∂θ = Σ_{n,d} (∂g/∂X_{n,d}) (∂X_{n,d}/∂θ) = ⟨grad_node, ∂X/∂θ⟩,
//! ```
//!
//! evaluated by [`chain_node_motion`].
//!
//! # `∂K_local/∂X` is **exact** (forward-mode AD of the element kernel)
//!
//! Rather than hand-derive the (correct but error-prone) analytic Jacobian
//! of `K_local = vol · (∇λ_p · ∇λ_q)` w.r.t. the twelve coordinates, we
//! evaluate the **same closed-form kernel** as
//! [`crate::assembly::electrostatic::tet_p1_local`] in dual-number
//! arithmetic (`Dual`) and read off the directional derivative. This is
//! **analytic** (exact forward-mode automatic differentiation — no
//! finite-difference truncation), so the adjoint-vs-FD test isolates the
//! correctness of the adjoint algebra + geometry chain, not the element
//! derivative. A dedicated unit test cross-checks the dual derivative
//! against a central finite difference of the real `f64` kernel.
//!
//! # Scope (honesty clause of #571)
//!
//! This is the **P1 scalar electrostatic** shape gradient — a full success
//! for the issue. It is restricted to the **voltage-driven** (`ρ = 0`)
//! regime, where the reduced RHS depends on geometry only through `K_fp`
//! (handled exactly above). The `ρ`-load shape term (`∂b_ρ/∂X`, the
//! consistent mass changing with volume) and the H(curl)/Nédélec extension
//! (its geometry factors are precomputed `&[f64]`, not yet on the tape) are
//! **noted follow-ons**, not attempted here.

use faer::Mat;
use faer::linalg::solvers::Solve;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::electrostatic::{
    EPS_0, Electrode, ElectrostaticError, assemble_electrostatic, tet_p1_local,
};
use crate::mesh::TetMesh;

pub mod conformal_basis;
pub mod floquet_element;
pub mod refined_capacitance;
pub mod transmon_morph;
pub mod transmon_remorph;

// ─────────────────────────────────────────────────────────────────────────
// Minimal forward-mode dual number for exact differentiation of the P1
// element-stiffness kernel w.r.t. a single seeded node coordinate.
// ─────────────────────────────────────────────────────────────────────────

/// A first-order **dual number** `re + du·ϵ` (`ϵ² = 0`) for exact
/// forward-mode automatic differentiation of the closed-form P1 element
/// kernel. Seeding one node coordinate with `du = 1` (all others `du = 0`)
/// and evaluating [`stiffness_bilinear_dual`] returns, in its `du` field,
/// the exact partial derivative of the element-stiffness bilinear form
/// w.r.t. that coordinate.
#[derive(Clone, Copy, Debug)]
struct Dual {
    re: f64,
    du: f64,
}

impl Dual {
    #[inline]
    fn cst(re: f64) -> Self {
        Self { re, du: 0.0 }
    }
    #[inline]
    fn var(re: f64) -> Self {
        Self { re, du: 1.0 }
    }
    #[inline]
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            du: self.du + o.du,
        }
    }
    #[inline]
    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            du: self.du - o.du,
        }
    }
    #[inline]
    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re,
            du: self.du * o.re + self.re * o.du,
        }
    }
    #[inline]
    fn div(self, o: Self) -> Self {
        let inv = 1.0 / o.re;
        Self {
            re: self.re * inv,
            du: (self.du * o.re - self.re * o.du) * inv * inv,
        }
    }
    #[inline]
    fn neg(self) -> Self {
        Self {
            re: -self.re,
            du: -self.du,
        }
    }
    /// `|x|`, with the sub-gradient at the (measure-zero) kink taken as the
    /// right derivative. The element determinant is bounded away from zero
    /// on any valid mesh, so the kink is never hit here.
    #[inline]
    fn abs(self) -> Self {
        if self.re >= 0.0 { self } else { self.neg() }
    }
    /// Multiply by an `f64` constant (a lifted scalar with zero tangent).
    #[inline]
    fn scale(self, c: f64) -> Self {
        Self {
            re: self.re * c,
            du: self.du * c,
        }
    }
}

#[inline]
fn dsub3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [a[0].sub(b[0]), a[1].sub(b[1]), a[2].sub(b[2])]
}
#[inline]
fn dcross3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [
        a[1].mul(b[2]).sub(a[2].mul(b[1])),
        a[2].mul(b[0]).sub(a[0].mul(b[2])),
        a[0].mul(b[1]).sub(a[1].mul(b[0])),
    ]
}
#[inline]
fn ddot3(a: [Dual; 3], b: [Dual; 3]) -> Dual {
    a[0].mul(b[0]).add(a[1].mul(b[1])).add(a[2].mul(b[2]))
}

/// The P1 element-stiffness **bilinear form** `Σ_{p,q} λ_p K_local[p][q] φ_q`
/// evaluated in dual arithmetic on dual-valued `coords`, so its `.du` is the
/// directional derivative of that scalar w.r.t. whichever coordinate was
/// seeded with `Dual::var`. Mirrors
/// [`crate::assembly::electrostatic::tet_p1_local`]'s stiffness exactly
/// (`K_ij = vol · ∇λ_i·∇λ_j`, `∇λ_i = g_i/det`, `vol = |det|/6`) so the
/// `.re` field reproduces the real `f64` element bilinear form.
fn stiffness_bilinear_dual(coords: &[[Dual; 3]; 4], lam: &[f64; 4], phi: &[f64; 4]) -> Dual {
    let v0 = coords[0];
    let e1 = dsub3(coords[1], v0);
    let e2 = dsub3(coords[2], v0);
    let e3 = dsub3(coords[3], v0);
    let g1 = dcross3(e2, e3);
    let g2 = dcross3(e3, e1);
    let g3 = dcross3(e1, e2);
    let det = ddot3(e1, g1); // signed 6V
    let vol = det.abs().scale(1.0 / 6.0);

    // Barycentric gradients ∇λ_i = g_i/det (i=1..3), ∇λ_0 = −Σ.
    let gl1 = [g1[0].div(det), g1[1].div(det), g1[2].div(det)];
    let gl2 = [g2[0].div(det), g2[1].div(det), g2[2].div(det)];
    let gl3 = [g3[0].div(det), g3[1].div(det), g3[2].div(det)];
    let gl0 = [
        gl1[0].add(gl2[0]).add(gl3[0]).neg(),
        gl1[1].add(gl2[1]).add(gl3[1]).neg(),
        gl1[2].add(gl2[2]).add(gl3[2]).neg(),
    ];
    let grads = [gl0, gl1, gl2, gl3];

    // Σ_{p,q} λ_p (vol · ∇λ_p·∇λ_q) φ_q, folding the f64 weights λ,φ in.
    let mut acc = Dual::cst(0.0);
    for p in 0..4 {
        let lp = lam[p];
        if lp == 0.0 {
            continue;
        }
        for q in 0..4 {
            let phiq = phi[q];
            if phiq == 0.0 {
                continue;
            }
            let kpq = vol.mul(ddot3(grads[p], grads[q]));
            acc = acc.add(kpq.scale(lp * phiq));
        }
    }
    acc
}

// ─────────────────────────────────────────────────────────────────────────
// Shape-gradient driver.
// ─────────────────────────────────────────────────────────────────────────

/// Result of an electrostatic discrete-adjoint **shape** gradient
/// evaluation.
#[derive(Debug, Clone)]
pub struct ShapeGradient {
    /// The scalar objective `g(φ)` at the (unperturbed) forward solution.
    pub objective: f64,
    /// The full **nodal-coordinate** gradient `∂g/∂X_{n,d}`, one `[x,y,z]`
    /// triple per node (length `mesh.n_nodes()`). Valid for **every** node
    /// — free and Dirichlet-pinned alike — since it is `−λᵀ(∂K/∂X)φ`, a
    /// well-defined function of any coordinate. Chain it through a
    /// node-motion map with [`chain_node_motion`] to obtain `∂g/∂θ`.
    pub grad_node: Vec<[f64; 3]>,
    /// Full-length `[n_nodes]` forward potential `φ` (pinned Dirichlet
    /// values in place), returned for post-processing / cross-checks.
    pub phi: Vec<f64>,
    /// Number of sparse LU **factorizations** performed. Always `1`: the
    /// forward and adjoint solves share a single factorization.
    pub n_factorizations: usize,
}

/// Compute the full nodal-coordinate gradient `∂g/∂X_{n,d}` of a scalar
/// electrostatic observable via the discrete adjoint — **one forward + one
/// adjoint solve**, reusing a single LU factorization — then chain through
/// any analytic node-motion map with [`chain_node_motion`].
///
/// This is the **voltage-driven** (`ρ = 0`) shape gradient: the design
/// enters only through the geometry-dependent stiffness `K(X)`. See the
/// module docs for the identity and the scope note.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh (fixed topology; the gradient is w.r.t. its
///   node positions).
/// * `eps_r` — per-tet relative permittivity (length `mesh.n_tets()`).
/// * `electrodes`, `ground` — Dirichlet boundary, exactly as
///   [`assemble_electrostatic`] takes them.
/// * `objective` — the scalar figure-of-merit; given the full-length nodal
///   potential `φ` it returns `(g, ∂g/∂φ)` with `∂g/∂φ` a full-length
///   `[n_nodes]` cotangent. Must not depend explicitly on geometry (only
///   through `φ`); its cotangent on pinned rows is ignored.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from assembly / factorization, and
/// returns [`ElectrostaticError::ShapeMismatch`] if the objective cotangent
/// has the wrong length.
pub fn electrostatic_shape_gradient<G>(
    mesh: &TetMesh,
    eps_r: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    objective: G,
) -> Result<ShapeGradient, ElectrostaticError>
where
    G: Fn(&[f64]) -> (f64, Vec<f64>),
{
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();

    // Voltage-driven: no volume charge. (ρ-load shape term is out of scope;
    // see module docs.)
    let rho = vec![0.0_f64; n_tets];

    // --- Assemble the SPD electrostatic system and factor ONCE. ---
    let sys = assemble_electrostatic(mesh, eps_r, &rho, electrodes, ground)?;
    let lu = sys
        .k
        .as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // --- Forward solve: K_ff φ_free = b_free. ---
    let mut fwd: Mat<f64> = Mat::from_fn(sys.n_free, 1, |i, _| sys.b[i]);
    lu.solve_in_place(fwd.as_mut());

    let mut phi = sys.dirichlet_value.clone();
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            phi[g] = fwd[(*fi, 0)];
        }
    }

    // --- Objective and its cotangent ∂g/∂φ. ---
    let (objective_value, dg_dphi) = objective(&phi);
    if dg_dphi.len() != n_nodes {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "objective cotangent length {} != node count {n_nodes}",
            dg_dphi.len()
        )));
    }

    // --- Adjoint solve: K_ffᵀ λ = (∂g/∂φ)_free, REUSING the forward
    // factorization via faer's transpose back-substitution (no refactor). ---
    let mut adj: Mat<f64> = Mat::from_fn(sys.n_free, 1, |_, _| 0.0);
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            adj[(*fi, 0)] = dg_dphi[g];
        }
    }
    lu.solve_transpose_in_place(adj.as_mut());

    // λ scattered to full length, zero on pinned rows.
    let mut lambda_full = vec![0.0_f64; n_nodes];
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            lambda_full[g] = adj[(*fi, 0)];
        }
    }

    // --- Nodal-coordinate gradient: ∂g/∂X_{n,d} = −Σ_t ε₀ ε_r[t]
    //     ∂/∂X_{n,d} (λ_localᵀ K_local(t) φ_local), evaluated by seeding each
    //     of a tet's 12 local coordinates and reading the dual tangent. ---
    let mut grad_node = vec![[0.0_f64; 3]; n_nodes];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let base = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let lam = [
            lambda_full[tet[0] as usize],
            lambda_full[tet[1] as usize],
            lambda_full[tet[2] as usize],
            lambda_full[tet[3] as usize],
        ];
        let phil = [
            phi[tet[0] as usize],
            phi[tet[1] as usize],
            phi[tet[2] as usize],
            phi[tet[3] as usize],
        ];
        // Skip a tet that cannot contribute (adjoint vanishes on all four
        // local rows ⇒ its stiffness never couples into the objective).
        if lam.iter().all(|&l| l == 0.0) {
            continue;
        }
        let eps_t = EPS_0 * eps_r[t];
        for a in 0..4 {
            let gn = &mut grad_node[tet[a] as usize];
            for (c, slot) in gn.iter_mut().enumerate() {
                // Seed local vertex a, axis c; all other coordinates are
                // constants (zero tangent).
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c] = Dual::var(base[a][c]);
                let d = stiffness_bilinear_dual(&dc, &lam, &phil).du;
                *slot -= eps_t * d;
            }
        }
    }

    Ok(ShapeGradient {
        objective: objective_value,
        grad_node,
        phi,
        n_factorizations,
    })
}

/// Chain a full nodal-coordinate gradient through a node-motion map
/// `θ ↦ X(θ)` to obtain the scalar design gradient
/// `∂g/∂θ = Σ_{n,d} (∂g/∂X_{n,d}) (∂X_{n,d}/∂θ) = ⟨grad_node, ∂X/∂θ⟩`.
///
/// `dnode_dtheta[n] = ∂X_n/∂θ` is the (analytic) velocity field of the map,
/// one `[x,y,z]` triple per node. For a map **linear** in `θ`
/// (`X(θ) = X⁰ + θ·D`) this velocity is the constant `D`, so the same array
/// both defines the finite-difference perturbation and the analytic
/// Jacobian.
///
/// # Panics
///
/// Panics if `grad_node` and `dnode_dtheta` differ in length.
pub fn chain_node_motion(grad_node: &[[f64; 3]], dnode_dtheta: &[[f64; 3]]) -> f64 {
    assert_eq!(
        grad_node.len(),
        dnode_dtheta.len(),
        "chain_node_motion: grad_node len {} != dnode_dtheta len {}",
        grad_node.len(),
        dnode_dtheta.len()
    );
    grad_node
        .iter()
        .zip(dnode_dtheta)
        .map(|(g, d)| g[0] * d[0] + g[1] * d[1] + g[2] * d[2])
        .sum()
}

// ─────────────────────────────────────────────────────────────────────────
// Analytic node-motion maps for shape optimization (issue #589).
// ─────────────────────────────────────────────────────────────────────────

/// Mean position (centroid) of a node subset — the fixed point of the
/// in-plane scale map ([`in_plane_scale_velocity`] /
/// [`apply_in_plane_scale`]).
///
/// # Panics
///
/// Panics if `subset` is empty or references a node outside the mesh.
pub fn subset_centroid(mesh: &TetMesh, subset: &[u32]) -> [f64; 3] {
    assert!(!subset.is_empty(), "subset_centroid: empty node subset");
    let mut c = [0.0_f64; 3];
    for &n in subset {
        let p = mesh.nodes[n as usize];
        c[0] += p[0];
        c[1] += p[1];
        c[2] += p[2];
    }
    let inv = 1.0 / subset.len() as f64;
    [c[0] * inv, c[1] * inv, c[2] * inv]
}

/// Velocity field `∂X/∂θ` of the **in-plane subset scale** map
///
/// ```text
///   X_n(θ) = c + (1 + θ)·(X⁰_n − c)   in x,y   (z unchanged),   n ∈ subset,
///   X_n(θ) = X⁰_n                                               otherwise,
/// ```
///
/// where `c` is the subset centroid ([`subset_centroid`]). The map is
/// **linear** in `θ`, so the constant velocity `∂X_n/∂θ = (X⁰_n − c)`
/// (in-plane components only, zero `z`) is exact at every `θ` — the same
/// array feeds both [`chain_node_motion`] /
/// [`CapacitanceShapeGradient::dc_dtheta`] and the finite-difference
/// perturbation. Off-subset nodes carry zero velocity (fixed topology,
/// fixed surroundings; adjacent tets deform).
///
/// This is the transmon **island-pad scale** parameterization of issue
/// #589: `subset` = the island conductor's node set, `1 + θ` = the pad's
/// in-plane linear scale factor.
///
/// # Panics
///
/// Panics if `subset` is empty or references a node outside the mesh.
pub fn in_plane_scale_velocity(mesh: &TetMesh, subset: &[u32]) -> Vec<[f64; 3]> {
    let c = subset_centroid(mesh, subset);
    let mut vel = vec![[0.0_f64; 3]; mesh.n_nodes()];
    for &n in subset {
        let p = mesh.nodes[n as usize];
        vel[n as usize] = [p[0] - c[0], p[1] - c[1], 0.0];
    }
    vel
}

/// Apply the in-plane subset scale map at parameter `θ` (see
/// [`in_plane_scale_velocity`]): returns a clone of `mesh` with each subset
/// node moved to `c + (1 + θ)(X⁰ − c)` in `x, y` (`z` and all off-subset
/// nodes unchanged, topology fixed). `θ = 0` reproduces the input mesh
/// exactly; `θ < 0` shrinks the subset about its centroid.
///
/// # Panics
///
/// Panics if `subset` is empty or references a node outside the mesh.
pub fn apply_in_plane_scale(mesh: &TetMesh, subset: &[u32], theta: f64) -> TetMesh {
    let c = subset_centroid(mesh, subset);
    let mut moved = mesh.clone();
    for &n in subset {
        let p = &mut moved.nodes[n as usize];
        p[0] = c[0] + (1.0 + theta) * (p[0] - c[0]);
        p[1] = c[1] + (1.0 + theta) * (p[1] - c[1]);
    }
    moved
}

/// Apply a **general** analytic node-motion map `X(θ) = X⁰ + θ·D` at
/// parameter `θ`: returns a clone of `mesh` with **every** node `n` moved to
/// `X⁰_n + θ·velocity[n]` (all three axes), topology fixed. The map is
/// exactly linear in `θ`, so `velocity` is simultaneously the finite-
/// difference perturbation and the analytic Jacobian `∂X/∂θ` consumed by
/// [`chain_node_motion`] / [`CapacitanceShapeGradient::dc_dtheta`].
///
/// This is the mesh-morphing counterpart of the subset-local
/// [`apply_in_plane_scale`]: where that map moves only the island nodes and
/// leaves the surroundings fixed (concentrating all strain in the adjacent
/// tets), `velocity` here is typically a **harmonic extension**
/// ([`harmonic_extension_velocity`]) that spreads the prescribed island
/// motion smoothly into the volume, so near-island tets deform proportionally
/// and the mesh-validity budget widens.
///
/// # Panics
///
/// Panics if `velocity.len() != mesh.n_nodes()`.
pub fn apply_node_motion(mesh: &TetMesh, velocity: &[[f64; 3]], theta: f64) -> TetMesh {
    assert_eq!(
        velocity.len(),
        mesh.n_nodes(),
        "apply_node_motion: velocity len {} != node count {}",
        velocity.len(),
        mesh.n_nodes()
    );
    let mut moved = mesh.clone();
    for (node, d) in moved.nodes.iter_mut().zip(velocity) {
        node[0] += theta * d[0];
        node[1] += theta * d[1];
        node[2] += theta * d[2];
    }
    moved
}

/// Assemble the full `n_nodes × n_nodes` P1 **Laplace** stiffness
/// `L_ij = ∫ ∇λ_i·∇λ_j` (unit coefficient, `ε_r ≡ 1`) on `mesh`, reusing the
/// same closed-form element kernel as the electrostatic assembly
/// ([`tet_p1_local`]). Used as the smoothing operator for the harmonic
/// mesh-morphing extension.
fn assemble_p1_laplace(mesh: &TetMesh) -> Result<SparseColMat<usize, f64>, ElectrostaticError> {
    let n = mesh.n_nodes();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
    for tet in &mesh.tets {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (k_local, _m, _v) = tet_p1_local(&coords);
        for p in 0..4 {
            let gp = tet[p] as usize;
            for q in 0..4 {
                trips.push(Triplet::new(gp, tet[q] as usize, k_local[p][q]));
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))
}

/// **Harmonic (Laplace-smoothed) mesh-morphing** velocity field `D = ∂X/∂θ`
/// that extends a prescribed island node-motion smoothly into the
/// surrounding mesh volume — the mesh-deformation upgrade of the rigid
/// island-only [`in_plane_scale_velocity`] (issue #594, building on #589).
///
/// # What it computes
///
/// The rigid map [`apply_in_plane_scale`] moves the island nodes by
/// `X⁰ − c` (in-plane scale about the centroid `c`) and leaves **all** other
/// nodes fixed, so the entire deformation strain is absorbed by the thin
/// layer of tets touching the island boundary — on the real transmon that
/// inverts the ~0.7 μm junction-region tets at a tiny shrink (`θ ≈ −0.0097`),
/// 33× short of the design anchor (#589/PR#590).
///
/// This routine instead solves a **P1 Laplace** problem (one scalar solve per
/// in-plane component, sharing a single factorization) for a smooth extension
/// field `D`:
///
/// ```text
///   ∇²D = 0              on the free volume,
///   D_n = X⁰_n − c       (in-plane, z-component 0) on the island   (Dirichlet),
///   D_n = 0              on `fixed_zero` (other conductors + far boundary).
/// ```
///
/// The island nodes therefore move **exactly** as under the rigid map (same
/// prescribed island shape change), but the surrounding free nodes now follow
/// a harmonic interpolation that decays to zero at the other conductors and
/// the outer domain boundary. Because the near-island free nodes move almost
/// as much as the island itself, the relative displacement **across** the
/// tiny junction-region tets is a fraction of the rigid map's, widening the
/// mesh-validity budget.
///
/// The returned field is exactly linear in `θ` (`X(θ) = X⁰ + θ·D`), so it
/// feeds both [`apply_node_motion`] (the finite-difference perturbation) and
/// [`chain_node_motion`] / the [`CapacitanceShapeGradient`] helpers (the
/// analytic `∂C/∂θ = ⟨grad_node, D⟩`) unchanged — the #589 gradient
/// contraction carries over verbatim, only the velocity field changes.
///
/// # Arguments
///
/// * `mesh` — the base tetrahedral mesh (the field is w.r.t. its nodes).
/// * `island` — the moving conductor's node set; each is Dirichlet-pinned to
///   its rigid in-plane scale velocity `X⁰ − c` (`c = ` island centroid).
/// * `fixed_zero` — nodes held at zero velocity: the **other** conductors
///   (ground / feedline — they must not move) and the far/outer boundary
///   (the domain edge stays put). Nodes in both `island` and `fixed_zero`
///   resolve to the island (moving) value.
///
/// # Errors
///
/// Propagates [`ElectrostaticError::Assembly`] / [`ElectrostaticError::Factorization`]
/// from the Laplace assembly and solve.
///
/// # Panics
///
/// Panics if `island` is empty or references a node outside the mesh (via
/// [`subset_centroid`] / [`in_plane_scale_velocity`]).
pub fn harmonic_extension_velocity(
    mesh: &TetMesh,
    island: &[u32],
    fixed_zero: &[u32],
) -> Result<Vec<[f64; 3]>, ElectrostaticError> {
    let n = mesh.n_nodes();

    // Prescribed island motion (rigid in-plane scale about the centroid):
    // this is the Dirichlet data on the island nodes.
    let rigid = in_plane_scale_velocity(mesh, island);

    // Dirichlet mask + per-node in-plane (x, y) Dirichlet values.
    let mut pinned = vec![false; n];
    let mut dval = vec![[0.0_f64; 2]; n];
    for &g in fixed_zero {
        pinned[g as usize] = true; // value stays [0, 0]
    }
    for &g in island {
        let gi = g as usize;
        pinned[gi] = true;
        dval[gi] = [rigid[gi][0], rigid[gi][1]];
    }

    // Free renumbering.
    let mut free_of = vec![None; n];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of[g] = Some(n_free);
            n_free += 1;
        }
    }

    // Assemble the Laplace smoothing operator once and reduce out the
    // Dirichlet rows/cols, folding the pinned values into the RHS. Both
    // in-plane components share this single reduced factorization.
    let l_full = assemble_p1_laplace(mesh)?;
    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
    let mut b: Mat<f64> = Mat::zeros(n_free, 2);
    {
        let l_ref = l_full.as_ref();
        let cp = l_ref.col_ptr();
        let row_idx = l_ref.row_idx();
        let vals = l_ref.val();
        for j in 0..n {
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                let v = vals[k];
                match (free_of[i], free_of[j]) {
                    (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                    (Some(fi), None) => {
                        // Column j pinned: move −K_fp·D_pinned to the RHS.
                        b[(fi, 0)] -= v * dval[j][0];
                        b[(fi, 1)] -= v * dval[j][1];
                    }
                    _ => {}
                }
            }
        }
    }

    let mut vel = vec![[0.0_f64; 3]; n];
    if n_free > 0 {
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        lu.solve_in_place(b.as_mut());
        for (g, slot) in free_of.iter().enumerate() {
            if let Some(fi) = slot {
                vel[g] = [b[(*fi, 0)], b[(*fi, 1)], 0.0];
            }
        }
    }
    // Scatter the Dirichlet (island + fixed_zero) values.
    for g in 0..n {
        if pinned[g] {
            vel[g] = [dval[g][0], dval[g][1], 0.0];
        }
    }
    Ok(vel)
}

/// **Harmonic extension of a prescribed node velocity**, one P1-Laplace
/// factorization and three right-hand sides (one per Cartesian component):
///
/// ```text
///   ∇²D = 0 on the free nodes,   D_n = v_n on `prescribed`,   D_n = 0 on `fixed_zero`,
/// ```
///
/// with every other node free (natural / Neumann). It is the single-solve
/// counterpart of summing [`FreeformBoundaryMorph::harmonic_boundary`] over
/// one column per prescribed node and axis — the same field by linearity —
/// whose cost (`3 × prescribed nodes` right-hand sides) grows with the
/// group size (issue #883; Epic #841 Phase 5a binds CLI shape parameters
/// through it). A node listed in both `prescribed` and `fixed_zero` stays
/// fixed (pinned wins). The field is exactly linear in the parameter, so
/// `X(θ) = X⁰ + θ·D` ([`apply_node_motion`]) is both the finite-difference
/// perturbation and the analytic `∂X/∂θ`.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] for an out-of-range node,
/// [`ElectrostaticError::Assembly`] / [`ElectrostaticError::Factorization`]
/// from the Laplace assembly and solve.
pub fn harmonic_dirichlet_velocity(
    mesh: &TetMesh,
    prescribed: &[(u32, [f64; 3])],
    fixed_zero: &[u32],
) -> Result<Vec<[f64; 3]>, ElectrostaticError> {
    let n = mesh.n_nodes();
    if let Some(&(g, _)) = prescribed.iter().find(|(g, _)| *g as usize >= n) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "prescribed node {g} out of range (n_nodes {n})"
        )));
    }
    if let Some(&g) = fixed_zero.iter().find(|g| **g as usize >= n) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "fixed_zero node {g} out of range (n_nodes {n})"
        )));
    }
    let mut pinned = vec![false; n];
    let mut dval = vec![[0.0_f64; 3]; n];
    for &(g, v) in prescribed {
        pinned[g as usize] = true;
        dval[g as usize] = v;
    }
    for &g in fixed_zero {
        pinned[g as usize] = true;
        dval[g as usize] = [0.0; 3];
    }
    let mut free_of = vec![None; n];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of[g] = Some(n_free);
            n_free += 1;
        }
    }
    let l_full = assemble_p1_laplace(mesh)?;
    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
    let mut b: Mat<f64> = Mat::zeros(n_free, 3);
    {
        let l_ref = l_full.as_ref();
        let cp = l_ref.col_ptr();
        let row_idx = l_ref.row_idx();
        let vals = l_ref.val();
        for j in 0..n {
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                let v = vals[k];
                match (free_of[i], free_of[j]) {
                    (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                    (Some(fi), None) => {
                        for c in 0..3 {
                            b[(fi, c)] -= v * dval[j][c];
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut vel = dval;
    if n_free > 0 {
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        lu.solve_in_place(b.as_mut());
        for (g, slot) in free_of.iter().enumerate() {
            if let Some(fi) = slot {
                vel[g] = [b[(*fi, 0)], b[(*fi, 1)], b[(*fi, 2)]];
            }
        }
    }
    Ok(vel)
}

/// Signed 6-volume (`det[e₁ e₂ e₃]`) of tet `t` of `mesh`.
fn tet_signed_6vol(mesh: &TetMesh, t: usize) -> f64 {
    let tet = mesh.tets[t];
    let v0 = mesh.nodes[tet[0] as usize];
    let e = |k: usize| -> [f64; 3] {
        let v = mesh.nodes[tet[k] as usize];
        [v[0] - v0[0], v[1] - v0[1], v[2] - v0[2]]
    };
    let (e1, e2, e3) = (e(1), e(2), e(3));
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

/// Mesh-distortion guard for node-motion maps: the minimum, over all tets,
/// of the **signed**-volume ratio `vol(moved tet) / vol(base tet)`.
///
/// * `≈ 1` — the map barely deformed that worst tet;
/// * `→ 0⁺` — a tet is nearly degenerate (discretization quality degrading);
/// * `≤ 0` — a tet **inverted**: the moved mesh is no longer a valid
///   discretization and any solve on it is meaningless. Callers doing real
///   shape optimization must reject such a step (the honesty clause of
///   issue #589: a large pad shrink may exceed the safe deformation of the
///   adjacent tets).
///
/// # Panics
///
/// Panics if the meshes differ in tet count (the maps here are fixed-topology).
pub fn min_tet_volume_ratio(base: &TetMesh, moved: &TetMesh) -> f64 {
    assert_eq!(
        base.n_tets(),
        moved.n_tets(),
        "min_tet_volume_ratio: tet counts differ (fixed-topology maps only)"
    );
    let mut worst = f64::INFINITY;
    for t in 0..base.n_tets() {
        let vb = tet_signed_6vol(base, t);
        let vm = tet_signed_6vol(moved, t);
        // vb ≠ 0 on any valid input mesh; the ratio keeps vb's orientation.
        worst = worst.min(vm / vb);
    }
    worst
}

/// [`min_tet_volume_ratio`] together with the index of the tet that attains
/// it (the first one, on ties), so a distortion budget can name the region
/// that limits it.
///
/// # Panics
///
/// Panics if the meshes differ in tet count or have no tets.
pub fn worst_tet_volume_ratio(base: &TetMesh, moved: &TetMesh) -> (f64, usize) {
    assert_eq!(
        base.n_tets(),
        moved.n_tets(),
        "worst_tet_volume_ratio: tet counts differ (fixed-topology maps only)"
    );
    assert!(base.n_tets() > 0, "worst_tet_volume_ratio: empty mesh");
    let mut worst = (f64::INFINITY, 0usize);
    for t in 0..base.n_tets() {
        let r = tet_signed_6vol(moved, t) / tet_signed_6vol(base, t);
        if r < worst.0 {
            worst = (r, t);
        }
    }
    worst
}

// ─────────────────────────────────────────────────────────────────────────
// Re-morphing support (Epic #569 / umbrella #1034, issue #1036, Phase B).
//
// A re-morphing optimizer rebuilds every parameter field `D_i(X_k)` about the
// current coordinates at each accepted iterate. Three pieces:
//
// * `assemble_p1_laplace_weighted`: the P1 Laplace operator with a per-tet
//   diffusivity, so a volume-stiffened morph (diffusivity `(V₀/V_k)^α`) can
//   push deformation away from tets that have already been compressed. The
//   unit-coefficient `assemble_p1_laplace` above is untouched; a unit test
//   checks the weighted sibling reproduces it bit-for-bit at weights of 1.
// * `harmonic_dirichlet_velocities`: several fields from ONE factorization.
//   That is valid only when every column pins the same node set (the
//   prescribed nodes plus `fixed_zero`), with only the values differing, so
//   the function rejects columns whose prescribed sets differ.
// * `tet_volume_ratios` / `tet_volume_ratio_derivatives`: the per-tet ratio
//   against the base mesh and its exact first derivative along a set of
//   node-motion fields, which linearize the mesh-quality constraint.
// ─────────────────────────────────────────────────────────────────────────

/// [`assemble_p1_laplace`] with a per-tet diffusivity: `L_ij = Σ_t w_t ∫_t
/// ∇λ_i·∇λ_j`. With every weight equal to 1 it reproduces the unit operator
/// bit-for-bit (the element kernel is the same; `x · 1.0 == x` exactly).
fn assemble_p1_laplace_weighted(
    mesh: &TetMesh,
    weights: &[f64],
) -> Result<SparseColMat<usize, f64>, ElectrostaticError> {
    if weights.len() != mesh.n_tets() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "Laplace diffusivity has {} entries, mesh has {} tets",
            weights.len(),
            mesh.n_tets()
        )));
    }
    if let Some((t, w)) = weights
        .iter()
        .enumerate()
        .find(|(_, w)| !(w.is_finite() && **w > 0.0))
    {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "Laplace diffusivity of tet {t} is {w}; it must be finite and positive"
        )));
    }
    let n = mesh.n_nodes();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
    for (tet, &w) in mesh.tets.iter().zip(weights) {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (k_local, _m, _v) = tet_p1_local(&coords);
        for p in 0..4 {
            let gp = tet[p] as usize;
            for q in 0..4 {
                trips.push(Triplet::new(gp, tet[q] as usize, w * k_local[p][q]));
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))
}

/// Several **harmonic extensions from one factorization**: column `c` solves
///
/// ```text
///   ∇·(w ∇D_c) = 0 on the free nodes,   D_c = v on `columns[c]`,   D_c = 0 on `fixed_zero`,
/// ```
///
/// with `w` the per-tet `diffusivity` (`None` = unit, the operator of
/// [`harmonic_dirichlet_velocity`]). As there, a node in both a column and
/// `fixed_zero` stays fixed.
///
/// One LU serves every column only because every column has the same
/// Dirichlet **set**: the function requires the prescribed node sets of all
/// columns to be identical (values may differ), and returns an error
/// otherwise. Fields whose pinned sets differ (on the transmon, `theta_G`
/// releases a ground band that `theta_L` and `theta_W` pin) must be built by
/// separate calls, one factorization per distinct set. The single-column,
/// unit-diffusivity call is the same field as [`harmonic_dirichlet_velocity`]
/// (to round-off; a unit test checks it).
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] for an out-of-range node, columns
/// with different prescribed node sets, a node prescribed twice in one
/// column, or a bad diffusivity; [`ElectrostaticError::Assembly`] /
/// [`ElectrostaticError::Factorization`] from the Laplace assembly and solve.
pub fn harmonic_dirichlet_velocities(
    mesh: &TetMesh,
    diffusivity: Option<&[f64]>,
    columns: &[Vec<(u32, [f64; 3])>],
    fixed_zero: &[u32],
) -> Result<Vec<Vec<[f64; 3]>>, ElectrostaticError> {
    let n = mesh.n_nodes();
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let node_set = |c: &[(u32, [f64; 3])]| -> Vec<u32> {
        let mut s: Vec<u32> = c.iter().map(|(g, _)| *g).collect();
        s.sort_unstable();
        s
    };
    let set0 = node_set(&columns[0]);
    if set0.windows(2).any(|w| w[0] == w[1]) {
        return Err(ElectrostaticError::ShapeMismatch(
            "harmonic_dirichlet_velocities: a node is prescribed twice in column 0".to_string(),
        ));
    }
    if let Some(&g) = set0.iter().find(|g| **g as usize >= n) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "prescribed node {g} out of range (n_nodes {n})"
        )));
    }
    for (c, col) in columns.iter().enumerate().skip(1) {
        if node_set(col) != set0 {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "harmonic_dirichlet_velocities: column {c} prescribes a different node set \
                 from column 0; one factorization is valid only for identical Dirichlet sets \
                 (build differing sets with separate calls)"
            )));
        }
    }
    if let Some(&g) = fixed_zero.iter().find(|g| **g as usize >= n) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "fixed_zero node {g} out of range (n_nodes {n})"
        )));
    }
    let n_cols = columns.len();
    let mut pinned = vec![false; n];
    let mut dval = vec![vec![[0.0_f64; 3]; n]; n_cols];
    for (c, col) in columns.iter().enumerate() {
        for &(g, v) in col {
            pinned[g as usize] = true;
            dval[c][g as usize] = v;
        }
    }
    for &g in fixed_zero {
        pinned[g as usize] = true;
        for d in dval.iter_mut() {
            d[g as usize] = [0.0; 3];
        }
    }
    let mut free_of = vec![None; n];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of[g] = Some(n_free);
            n_free += 1;
        }
    }
    let l_full = match diffusivity {
        None => assemble_p1_laplace(mesh)?,
        Some(w) => assemble_p1_laplace_weighted(mesh, w)?,
    };
    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
    let mut b: Mat<f64> = Mat::zeros(n_free, 3 * n_cols);
    {
        let l_ref = l_full.as_ref();
        let cp = l_ref.col_ptr();
        let row_idx = l_ref.row_idx();
        let vals = l_ref.val();
        for j in 0..n {
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                let v = vals[k];
                match (free_of[i], free_of[j]) {
                    (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                    (Some(fi), None) => {
                        for (c, d) in dval.iter().enumerate() {
                            for a in 0..3 {
                                b[(fi, 3 * c + a)] -= v * d[j][a];
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut vels = dval;
    if n_free > 0 {
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
            .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        lu.solve_in_place(b.as_mut());
        for (c, vel) in vels.iter_mut().enumerate() {
            for (g, slot) in free_of.iter().enumerate() {
                if let Some(fi) = slot {
                    vel[g] = [b[(*fi, 3 * c)], b[(*fi, 3 * c + 1)], b[(*fi, 3 * c + 2)]];
                }
            }
        }
    }
    Ok(vels)
}

/// The signed-volume ratio `vol(moved tet) / vol(base tet)` of every tet
/// (the per-tet values behind [`min_tet_volume_ratio`]).
///
/// # Panics
///
/// Panics if the meshes differ in tet count.
pub fn tet_volume_ratios(base: &TetMesh, moved: &TetMesh) -> Vec<f64> {
    assert_eq!(
        base.n_tets(),
        moved.n_tets(),
        "tet_volume_ratios: tet counts differ (fixed-topology maps only)"
    );
    (0..base.n_tets())
        .map(|t| tet_signed_6vol(moved, t) / tet_signed_6vol(base, t))
        .collect()
}

/// Volume-stiffening diffusivity `w_t = (V₀_t / V_t)^α` for a re-morph about
/// `current`: tets that `current` has already compressed relative to `base`
/// get a stiffer Laplace coefficient, so the next field deforms them less.
/// `α = 0` gives the unit operator.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] if `alpha` is negative or not
/// finite, or if any tet of `current` is inverted or degenerate.
pub fn volume_stiffening_weights(
    base: &TetMesh,
    current: &TetMesh,
    alpha: f64,
) -> Result<Vec<f64>, ElectrostaticError> {
    if !(alpha.is_finite() && alpha >= 0.0) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "volume_stiffening_weights: alpha {alpha} must be finite and non-negative"
        )));
    }
    let ratios = tet_volume_ratios(base, current);
    if let Some((t, r)) = ratios.iter().enumerate().find(|(_, r)| **r <= 0.0) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "volume_stiffening_weights: tet {t} has volume ratio {r} (inverted)"
        )));
    }
    Ok(ratios.into_iter().map(|r| r.powf(-alpha)).collect())
}

/// Exact first derivatives of the tet volume ratio `r_t = V_t(X)/V₀_t` at
/// `current` along node-motion fields: entry `[k][i]` is
/// `d r_{tets[k]} / dθ_i` for `X = current + Σ_i θ_i fields[i]` at `θ = 0`.
/// Each derivative is the closed form `d(6V) = Σ_a (∂det/∂e_a)·(D_a − D_0)`
/// with `∂det/∂e_1 = e_2×e_3` and its cyclic shifts; a unit test checks it
/// against central differences.
///
/// # Panics
///
/// Panics if the meshes differ in tet count, a tet index is out of range, or
/// a field's length differs from the node count.
pub fn tet_volume_ratio_derivatives(
    base: &TetMesh,
    current: &TetMesh,
    tets: &[usize],
    fields: &[&[[f64; 3]]],
) -> Vec<Vec<f64>> {
    assert_eq!(
        base.n_tets(),
        current.n_tets(),
        "tet_volume_ratio_derivatives: tet counts differ"
    );
    for f in fields {
        assert_eq!(
            f.len(),
            current.n_nodes(),
            "tet_volume_ratio_derivatives: field length != node count"
        );
    }
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    tets.iter()
        .map(|&t| {
            let tet = current.tets[t];
            let x = |k: usize| current.nodes[tet[k] as usize];
            let (e1, e2, e3) = (sub(x(1), x(0)), sub(x(2), x(0)), sub(x(3), x(0)));
            let (c1, c2, c3) = (cross(e2, e3), cross(e3, e1), cross(e1, e2));
            let v0 = tet_signed_6vol(base, t);
            fields
                .iter()
                .map(|f| {
                    let d = |k: usize| f[tet[k] as usize];
                    (dot(c1, sub(d(1), d(0))) + dot(c2, sub(d(2), d(0))) + dot(c3, sub(d(3), d(0))))
                        / v0
                })
                .collect()
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// High-DOF freeform boundary shape parametrization + mesh-morph regularizer
// (Epic #647 Phase 1, issue #648).
//
// The discrete-adjoint drivers above (electrostatic / capacitance here, and the
// driven-Nédélec `crate::driven::shape` capstone) all return the SAME currency:
// a full nodal-coordinate gradient `grad_node = ∂g/∂X_{n,d}`, contracted against
// a node-motion field by `chain_node_motion`. A **single** design DOF is one
// such field; a **freeform, high-DOF** boundary parametrization is simply a
// *stack* of them — a linear map `X ↦ ΣX_p D_p` from a design vector to node
// motion, whose per-column contraction `⟨grad_node, D_p⟩` is the design
// gradient. Because the contraction reuses `chain_node_motion` verbatim, the
// existing single-DOF path is recovered exactly as the P = 1 special case: the
// upstream `grad_node` (and hence the #636 capstone gradient) is never touched.
//
// The columns are built by the same **harmonic (Laplace) mesh-morph** as
// `harmonic_extension_velocity`: prescribed boundary-node motions are extended
// smoothly into the interior so the volumetric tets stay valid (non-inverted)
// under large deformation. `min_tet_volume_ratio` is the guard.
// ─────────────────────────────────────────────────────────────────────────

/// One design degree of freedom of a [`FreeformBoundaryMorph`]: a prescribed
/// motion of a single **boundary node** along a fixed direction.
///
/// A freeform curved-metal boundary is parametrized by many of these — one (or
/// several, for independent axes) per free boundary node — each an independent
/// column of the morph. The `dir` need not be a unit vector; its magnitude sets
/// the DOF's length scale (e.g. an outward surface normal scaled to the local
/// mesh size).
#[derive(Clone, Copy, Debug)]
pub struct BoundaryMotionDof {
    /// The boundary node this DOF moves (index into `mesh.nodes`).
    pub node: u32,
    /// The world-frame motion of `node` per unit design value `∂X_node/∂X_p`.
    pub dir: [f64; 3],
}

/// A **high-DOF freeform boundary shape parametrization** with an interior
/// **mesh-morph regularizer** — the many-DOF generalization of a single
/// node-motion map (Epic #647 Phase 1, issue #648).
///
/// It is a *linear* map from a design vector `X ∈ ℝ^P` to a full
/// nodal-coordinate displacement field
///
/// ```text
///   X_node(X) = X⁰_node + Σ_p X_p · D_p,
/// ```
///
/// where each column `D_p = ∂X_node/∂X_p` (one `[x, y, z]` triple per node,
/// length `n_nodes`) is the velocity field of design DOF `p`. Because the map is
/// linear in `X`, each `D_p` is **simultaneously** the finite-difference
/// perturbation direction and the analytic Jacobian consumed by
/// [`chain_node_motion`] — so the design gradient is the per-column contraction
///
/// ```text
///   ∂g/∂X_p = ⟨grad_node, D_p⟩,
/// ```
///
/// against **any** discrete-adjoint driver's nodal gradient: the scalar
/// [`ShapeGradient`] / [`CapacitanceShapeGradient`] here, or the driven-Nédélec
/// [`crate::driven::shape::DrivenShapeGradient`] capstone. The upstream
/// `grad_node` is never modified, so the **single-DOF path is recovered exactly
/// as the `P = 1` special case** ([`FreeformBoundaryMorph::from_columns`] with
/// one column reproduces `chain_node_motion` bit-for-bit).
///
/// # Two ways to build the columns
///
/// * [`FreeformBoundaryMorph::from_columns`] — an arbitrary **low-rank morph
///   basis** (each column a prescribed node-motion field). This is the general
///   escape hatch (control-point bases, PCA modes, or an externally supplied
///   single-DOF velocity for the regression check).
/// * [`FreeformBoundaryMorph::harmonic_boundary`] — the **mesh-morph
///   regularizer**: each freeform boundary-node motion ([`BoundaryMotionDof`])
///   is extended into the interior by a P1-Laplace solve (spring-analogy /
///   Laplacian smoothing), so the volumetric tets deform gracefully and stay
///   non-inverted under large boundary deformation. All columns share one
///   factorization.
///
/// The distortion budget is checked with [`FreeformBoundaryMorph::min_volume_ratio`]
/// (a wrapper over [`min_tet_volume_ratio`]): `> 0` ⇒ every tet is still valid.
#[derive(Debug, Clone)]
pub struct FreeformBoundaryMorph {
    /// One velocity field `D_p` per design DOF (each length `n_nodes`).
    columns: Vec<Vec<[f64; 3]>>,
}

impl FreeformBoundaryMorph {
    /// Build a morph from an explicit **low-rank basis**: `columns[p]` is the
    /// node-motion velocity field `D_p = ∂X_node/∂X_p` of design DOF `p` (length
    /// `n_nodes`). All columns must share the same length.
    ///
    /// This is the general constructor (control-point bases, reduced modes, or a
    /// single externally supplied velocity field). With exactly one column it is
    /// the identity wrapper around a single node-motion map: `design_gradient`
    /// returns `[chain_node_motion(grad_node, columns[0])]`.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if the columns differ in length.
    pub fn from_columns(columns: Vec<Vec<[f64; 3]>>) -> Result<Self, ElectrostaticError> {
        if let Some(first) = columns.first() {
            let n = first.len();
            for (p, c) in columns.iter().enumerate() {
                if c.len() != n {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "morph column {p} length {} != column 0 length {n}",
                        c.len()
                    )));
                }
            }
        }
        Ok(Self { columns })
    }

    /// Build the columns by the **harmonic (Laplace) mesh-morph regularizer**:
    /// each design DOF prescribes one boundary node's motion (`dofs[p]`), and the
    /// motion is extended smoothly into the interior by solving a P1-Laplace
    /// problem
    ///
    /// ```text
    ///   ∇²D_p = 0            on the free volume,
    ///   D_p = dir_p          at node_p                        (Dirichlet),
    ///   D_p = 0              at every OTHER DOF node and every `fixed_zero` node.
    /// ```
    ///
    /// so column `p` moves boundary node `p` (and, harmonically, its neighborhood)
    /// while holding every other design boundary node and every pinned node
    /// (`fixed_zero`: other conductors, the far/outer boundary, and — for the
    /// driven capstone — the PML shell and pinned feed) fixed. Because the near-DOF
    /// interior nodes follow the boundary, the relative strain across the adjacent
    /// tets is a fraction of a rigid boundary bump's, so the mesh stays valid under
    /// a much larger deformation (see [`harmonic_extension_velocity`] for the same
    /// mechanism on the single-DOF island map). All `P` columns (three RHS each)
    /// share a single Laplace factorization.
    ///
    /// The columns are exactly **zero** on every `fixed_zero` node, so a downstream
    /// contraction through [`crate::driven::shape::chain_node_motion_pml_pinned`]
    /// (which asserts zero
    /// motion on the pinned PML shell) is satisfied by construction whenever the
    /// PML nodes are listed in `fixed_zero`.
    ///
    /// # Arguments
    ///
    /// * `mesh` — base tetrahedral mesh (fixed topology; the columns are w.r.t.
    ///   its node positions).
    /// * `dofs` — the freeform boundary DOFs (must be non-empty); each moves one
    ///   node along its `dir`.
    /// * `fixed_zero` — nodes held at zero motion in every column (other
    ///   conductors, the outer/far boundary, and any pinned region such as the
    ///   UPML shell or a pinned feed). A node appearing in both `dofs` and
    ///   `fixed_zero` resolves to its (moving) DOF value in its own column.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if `dofs` is empty or references a
    /// node outside the mesh; [`ElectrostaticError::Assembly`] /
    /// [`ElectrostaticError::Factorization`] from the Laplace assembly / solve.
    pub fn harmonic_boundary(
        mesh: &TetMesh,
        dofs: &[BoundaryMotionDof],
        fixed_zero: &[u32],
    ) -> Result<Self, ElectrostaticError> {
        let n = mesh.n_nodes();
        let p = dofs.len();
        if p == 0 {
            return Err(ElectrostaticError::ShapeMismatch(
                "harmonic_boundary: at least one boundary DOF required".to_string(),
            ));
        }
        for (i, d) in dofs.iter().enumerate() {
            if d.node as usize >= n {
                return Err(ElectrostaticError::ShapeMismatch(format!(
                    "boundary DOF {i} node {} out of range (n_nodes {n})",
                    d.node
                )));
            }
        }
        for &g in fixed_zero {
            if g as usize >= n {
                return Err(ElectrostaticError::ShapeMismatch(format!(
                    "fixed_zero node {g} out of range (n_nodes {n})"
                )));
            }
        }

        // Pinned (Dirichlet) mask: every DOF node and every fixed_zero node.
        let mut pinned = vec![false; n];
        for &g in fixed_zero {
            pinned[g as usize] = true;
        }
        for d in dofs {
            pinned[d.node as usize] = true;
        }

        // Free renumbering.
        let mut free_of = vec![None; n];
        let mut n_free = 0usize;
        for (g, &pn) in pinned.iter().enumerate() {
            if !pn {
                free_of[g] = Some(n_free);
                n_free += 1;
            }
        }

        // Reduce the P1-Laplace operator once, building the 3P RHS by folding the
        // per-column Dirichlet data (nonzero only at that column's DOF node) into
        // `−K_fp · D_pinned`. Column `3p + c` is DOF `p`, spatial component `c`.
        let l_full = assemble_p1_laplace(mesh)?;
        let ncols = 3 * p;
        let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(mesh.n_tets() * 16);
        let mut b: Mat<f64> = Mat::zeros(n_free, ncols);
        {
            let l_ref = l_full.as_ref();
            let cp = l_ref.col_ptr();
            let row_idx = l_ref.row_idx();
            let vals = l_ref.val();
            for j in 0..n {
                for k in cp[j]..cp[j + 1] {
                    let i = row_idx[k];
                    let v = vals[k];
                    match (free_of[i], free_of[j]) {
                        (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                        (Some(fi), None) => {
                            // Column j is Dirichlet: it drives only the DOFs whose
                            // node IS j (others pin j to zero, contributing nothing).
                            for (pi, d) in dofs.iter().enumerate() {
                                if d.node as usize == j {
                                    for (c, &dc) in d.dir.iter().enumerate() {
                                        b[(fi, 3 * pi + c)] -= v * dc;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut columns = vec![vec![[0.0_f64; 3]; n]; p];
        if n_free > 0 {
            let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
                .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;
            let lu = k
                .as_ref()
                .sp_lu()
                .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
            lu.solve_in_place(b.as_mut());
            for (g, slot) in free_of.iter().enumerate() {
                if let Some(fi) = slot {
                    for (pi, col) in columns.iter_mut().enumerate() {
                        col[g] = [b[(*fi, 3 * pi)], b[(*fi, 3 * pi + 1)], b[(*fi, 3 * pi + 2)]];
                    }
                }
            }
        }
        // Scatter the Dirichlet motion: each DOF node moves by its `dir` in its
        // OWN column (every other pinned node stays [0, 0, 0], already initialized).
        for (pi, d) in dofs.iter().enumerate() {
            columns[pi][d.node as usize] = d.dir;
        }

        Ok(Self { columns })
    }

    /// Number of design DOFs `P` (columns).
    pub fn n_dofs(&self) -> usize {
        self.columns.len()
    }

    /// Number of mesh nodes each column spans (0 for an empty morph).
    pub fn n_nodes(&self) -> usize {
        self.columns.first().map_or(0, |c| c.len())
    }

    /// The velocity field `D_p = ∂X_node/∂X_p` of design DOF `p` (length
    /// `n_nodes`) — the finite-difference perturbation direction of that DOF.
    ///
    /// # Panics
    ///
    /// Panics if `dof >= n_dofs()`.
    pub fn velocity(&self, dof: usize) -> &[[f64; 3]] {
        &self.columns[dof]
    }

    /// The combined node-motion field `Σ_p X_p D_p` for a design vector `x`
    /// (length `n_nodes`) — the total displacement `X_node(x) − X⁰_node`.
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != n_dofs()`.
    pub fn combined_velocity(&self, x: &[f64]) -> Vec<[f64; 3]> {
        assert_eq!(
            x.len(),
            self.columns.len(),
            "combined_velocity: design vector len {} != n_dofs {}",
            x.len(),
            self.columns.len()
        );
        let mut v = vec![[0.0_f64; 3]; self.n_nodes()];
        for (&xp, col) in x.iter().zip(&self.columns) {
            if xp == 0.0 {
                continue;
            }
            for (acc, d) in v.iter_mut().zip(col) {
                acc[0] += xp * d[0];
                acc[1] += xp * d[1];
                acc[2] += xp * d[2];
            }
        }
        v
    }

    /// Apply the design vector `x`: returns a clone of `mesh` morphed to
    /// `X⁰ + Σ_p x_p D_p` (topology fixed). `x = 0` reproduces `mesh` exactly.
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != n_dofs()` or `mesh.n_nodes() != n_nodes()`.
    pub fn apply(&self, mesh: &TetMesh, x: &[f64]) -> TetMesh {
        let v = self.combined_velocity(x);
        apply_node_motion(mesh, &v, 1.0)
    }

    /// Mesh-distortion guard for a design vector: the minimum signed-volume
    /// ratio `vol(morphed)/vol(base)` over all tets after applying `x` (see
    /// [`min_tet_volume_ratio`]). `> 0` ⇒ every tet is still valid (non-inverted);
    /// `≤ 0` ⇒ the morph inverted a tet and must be rejected.
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != n_dofs()` or `mesh.n_nodes() != n_nodes()`.
    pub fn min_volume_ratio(&self, mesh: &TetMesh, x: &[f64]) -> f64 {
        let moved = self.apply(mesh, x);
        min_tet_volume_ratio(mesh, &moved)
    }

    /// The full **design-space gradient** `∂g/∂X = [⟨grad_node, D_p⟩]_p` (length
    /// `n_dofs`), contracting the upstream nodal gradient `grad_node` (from any
    /// discrete-adjoint driver) against every column with [`chain_node_motion`].
    ///
    /// # Panics
    ///
    /// Panics if `grad_node.len() != n_nodes()` (via [`chain_node_motion`]).
    pub fn design_gradient(&self, grad_node: &[[f64; 3]]) -> Vec<f64> {
        self.columns
            .iter()
            .map(|col| chain_node_motion(grad_node, col))
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Differentiable capacitance → E_C chain (Epic #476 / #569, issue #583).
// ─────────────────────────────────────────────────────────────────────────
//
// The capacitance observable is the stored field energy at unit excitation,
// `W = ½ φᵀ K(X) φ`, so `C_self = 2 W = φᵀ K φ`. Its geometry derivative has
// TWO parts (mirroring #577's `∂b/∂X` finding):
//
//   * an **implicit** part, `∂W/∂φ · dφ/dX`, recovered by the SAME discrete
//     adjoint as [`electrostatic_shape_gradient`] with cotangent
//     `∂W/∂φ = K φ`; and
//   * an **explicit** part, `∂W/∂X|_φ = ½ φᵀ (∂K/∂X) φ`, the direct
//     dependence of the energy on the node-coordinate-dependent stiffness.
//
// The explicit part is load-bearing here in the strongest possible sense:
// at the forward solution the free rows of the cotangent `K φ` vanish
// (`K_ff φ_free + K_fp φ_pinned = 0` is exactly the equilibrium), so the
// adjoint `λ ≈ 0` and the implicit part is ~round-off. The *entire*
// capacitance derivative is the explicit `∂K/∂X` energy term — dropping it
// (as the #577 mutation does) collapses `∂C/∂X` to ~0 and breaks the FD
// match. We still compute the adjoint faithfully (one forward + one adjoint
// solve, single factorization) so the structure generalizes and so a test
// can assert the implicit part is negligible relative to the explicit one.

/// Full-`K` sparse matrix-vector product `w = K_full · φ` (both full-length
/// `[n_nodes]`). Used to form the energy observable's cotangent
/// `∂(½φᵀKφ)/∂φ = Kφ`.
fn kfull_matvec(k: &SparseColMat<usize, f64>, phi: &[f64]) -> Vec<f64> {
    crate::sparse_ops::spmv(k.as_ref(), phi)
}

/// Result of the differentiable **capacitance** shape-gradient evaluation:
/// `∂C_self/∂X` for the electrostatic field-energy observable, via one
/// forward + one adjoint solve + a single geometry sweep (one LU
/// factorization).
#[derive(Debug, Clone)]
pub struct CapacitanceShapeGradient {
    /// Self-capacitance `C_self = φᵀ K φ = 2·field_energy` (F), for a
    /// **unit-voltage** electrode excitation (the fixture pins the electrode
    /// at 1 V, so `C_self` is the capacitance in farads). Equals the
    /// transmon `C_Σ` for a single island — the scope of this issue.
    pub c_self: f64,
    /// Stored field energy `W = ½ φᵀ K φ` (J) at the forward solution.
    pub field_energy: f64,
    /// Full nodal-coordinate gradient `∂C_self/∂X_{n,d}` (F/m), one `[x,y,z]`
    /// triple per node. This is `∂C`, not `∂(½C)` — the factor of two is
    /// already folded in. Chain through a node-motion map with
    /// [`CapacitanceShapeGradient::dc_dtheta`] (or [`chain_node_motion`]).
    pub grad_node_c: Vec<[f64; 3]>,
    /// Full nodal-coordinate gradient of the **implicit (adjoint) part**
    /// alone, `∂C_self/∂X` restricted to `2·∂W/∂φ·dφ/dX`. At the energy
    /// stationary point this is ~round-off; exposed so a test can assert the
    /// explicit `∂K/∂X` term carries the derivative (mutation resistance).
    pub grad_node_c_implicit: Vec<[f64; 3]>,
    /// Forward potential `φ` (full length, Dirichlet values in place).
    pub phi: Vec<f64>,
    /// LU factorizations performed — always `1` (forward + adjoint share it).
    pub n_factorizations: usize,
}

impl CapacitanceShapeGradient {
    /// `∂C_self/∂θ` for a node-motion map with velocity field
    /// `dnode_dtheta[n] = ∂X_n/∂θ` (one `[x,y,z]` triple per node).
    pub fn dc_dtheta(&self, dnode_dtheta: &[[f64; 3]]) -> f64 {
        chain_node_motion(&self.grad_node_c, dnode_dtheta)
    }

    /// `∂C_self/∂θ` from the **implicit (adjoint) part only** — a diagnostic
    /// that is ~0 at the energy stationary point.
    pub fn dc_dtheta_implicit(&self, dnode_dtheta: &[[f64; 3]]) -> f64 {
        chain_node_motion(&self.grad_node_c_implicit, dnode_dtheta)
    }

    /// `∂(E_C/h)/∂θ` (Hz per unit θ) for a node-motion map, composing the
    /// capacitance gradient with the analytic charging-energy chain factor
    /// `∂E_C/∂C_Σ = −e²/(2 C_Σ² h)` and treating `C_Σ = c_self` (single
    /// island) as the **total** capacitance, with no junction capacitance.
    /// For a transmon whose junction `C_J` is in parallel (issue #1066),
    /// compose [`crate::quantum::transmon::d_e_c_hz_from_c_sigma_d_c_sigma`]
    /// with [`Self::dc_dtheta`] instead.
    pub fn de_c_hz_dtheta(&self, dnode_dtheta: &[[f64; 3]]) -> f64 {
        crate::quantum::transmon::d_e_c_hz_d_c_sigma(self.c_self) * self.dc_dtheta(dnode_dtheta)
    }
}

/// Differentiable **capacitance** shape gradient `∂C_self/∂X` for the
/// voltage-driven electrostatic system, via the field-energy adjoint plus
/// the explicit `∂K/∂X` energy term — **one forward + one adjoint solve**,
/// reusing a single LU factorization, then a single geometry sweep.
///
/// The observable is the stored field energy `W = ½ φᵀ K(X) φ`, so
/// `C_self = 2 W = φᵀ K φ` at unit excitation. The returned
/// [`CapacitanceShapeGradient::grad_node_c`] is the full nodal-coordinate
/// gradient `∂C_self/∂X`; chain it through any analytic node-motion map with
/// [`chain_node_motion`] / the [`CapacitanceShapeGradient`] helpers to get
/// `∂C_self/∂θ` and `∂(E_C/h)/∂θ`.
///
/// # The two terms
///
/// `dW/dX = (∂W/∂φ · dφ/dX) + ∂W/∂X|_φ`. The first (implicit) term is the
/// discrete adjoint with cotangent `∂W/∂φ = K φ`; the second (explicit) term
/// is `½ φᵀ (∂K/∂X) φ`, evaluated by the same exact `Dual`-through-the-P1-
/// kernel machinery as `∂K/∂X`. **Both are required** — see the module-level
/// note; omitting the explicit term collapses the capacitance gradient to
/// ~0 (the adjoint alone vanishes at the energy stationary point).
///
/// # Arguments
///
/// Identical to [`assemble_electrostatic`] (charge-free / voltage-driven):
/// `mesh`, per-tet `eps_r`, the Dirichlet `electrodes` (pin the excited
/// conductor at 1 V for `c_self` to be the capacitance), and `ground`.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from assembly / factorization.
pub fn capacitance_shape_gradient(
    mesh: &TetMesh,
    eps_r: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<CapacitanceShapeGradient, ElectrostaticError> {
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();

    // Voltage-driven: no volume charge (ρ-load shape term is out of scope).
    let rho = vec![0.0_f64; n_tets];

    // --- Assemble the SPD system and factor ONCE. ---
    let sys = assemble_electrostatic(mesh, eps_r, &rho, electrodes, ground)?;
    let lu = sys
        .k
        .as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // --- Forward solve: K_ff φ_free = b_free. ---
    let mut fwd: Mat<f64> = Mat::from_fn(sys.n_free, 1, |i, _| sys.b[i]);
    lu.solve_in_place(fwd.as_mut());
    let mut phi = sys.dirichlet_value.clone();
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            phi[g] = fwd[(*fi, 0)];
        }
    }

    let field_energy = sys.field_energy(&phi);
    let c_self = 2.0 * field_energy;

    // --- Energy-observable cotangent ∂W/∂φ = K_full φ (full length). At the
    //     solution its FREE rows are ~0 (equilibrium), so λ is ~round-off. ---
    let kphi = kfull_matvec(&sys.k_full, &phi);

    // --- Adjoint solve K_ffᵀ λ = (K φ)_free, REUSING the forward
    //     factorization (transpose back-substitution — no refactor). ---
    let mut adj: Mat<f64> = Mat::from_fn(sys.n_free, 1, |_, _| 0.0);
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            adj[(*fi, 0)] = kphi[g];
        }
    }
    lu.solve_transpose_in_place(adj.as_mut());
    let mut lambda_full = vec![0.0_f64; n_nodes];
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            lambda_full[g] = adj[(*fi, 0)];
        }
    }

    // --- Geometry sweep. Per node coordinate, combine the two ∂(½C)/∂X
    //     contributions and scale by 2 for ∂C/∂X:
    //       implicit(½C) = −Σ_t ε₀ε_r ∂/∂X (λ_localᵀ K_local φ_local)
    //       explicit(½C) = +½ Σ_t ε₀ε_r ∂/∂X (φ_localᵀ K_local φ_local)
    //     using the exact dual tangent of the P1 element kernel. ---
    let mut grad_node_c = vec![[0.0_f64; 3]; n_nodes];
    let mut grad_node_c_implicit = vec![[0.0_f64; 3]; n_nodes];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let base = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let lam = [
            lambda_full[tet[0] as usize],
            lambda_full[tet[1] as usize],
            lambda_full[tet[2] as usize],
            lambda_full[tet[3] as usize],
        ];
        let phil = [
            phi[tet[0] as usize],
            phi[tet[1] as usize],
            phi[tet[2] as usize],
            phi[tet[3] as usize],
        ];
        let eps_t = EPS_0 * eps_r[t];
        for a in 0..4 {
            let node = tet[a] as usize;
            for c in 0..3 {
                // Seed local vertex a, axis c; all other coords constant.
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c] = Dual::var(base[a][c]);
                // Implicit: ∂(½C)/∂X = −ε₀ε_r ∂(λᵀK_localφ)/∂X.
                let d_impl = stiffness_bilinear_dual(&dc, &lam, &phil).du;
                let implicit_half = -eps_t * d_impl;
                // Explicit: ∂(½C)/∂X = ½ ε₀ε_r ∂(φᵀK_localφ)/∂X.
                let d_expl = stiffness_bilinear_dual(&dc, &phil, &phil).du;
                let explicit_half = 0.5 * eps_t * d_expl;
                grad_node_c[node][c] += 2.0 * (implicit_half + explicit_half);
                grad_node_c_implicit[node][c] += 2.0 * implicit_half;
            }
        }
    }

    Ok(CapacitanceShapeGradient {
        c_self,
        field_energy,
        grad_node_c,
        grad_node_c_implicit,
        phi,
        n_factorizations,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Full Maxwell capacitance-matrix shape gradient (Epic #569 / #1034, issue
// #1035).
// ─────────────────────────────────────────────────────────────────────────
//
// Every Maxwell entry is `C_ij = φ_iᵀ K φ_j` (`extract_capacitance`), with
// `φ_i` the unit excitation of conductor i (conductor i at 1 V, every other
// conductor and the ground at 0 V). Both fields are discrete-harmonic: their
// free rows satisfy `(K φ)_free = 0`. A geometry change leaves the Dirichlet
// VALUES fixed (1 or 0), so `dφ_i` is supported on the free rows only, and
// both implicit terms vanish exactly:
//
//   dφ_iᵀ K φ_j = Σ_free dφ_i (K φ_j)_free = 0,   φ_iᵀ K dφ_j = 0  (K symmetric).
//
// Hence `dC_ij/dX = φ_iᵀ (∂K/∂X) φ_j` — the explicit term alone, off-diagonal
// entries included. This extends the #583 stationarity result from `C_ii` to
// the whole matrix. The unit tests check it against central finite
// differences rather than assuming it.

/// Which bilinear sweep [`capacitance_matrix_sweep`] performs. Only
/// [`MatrixSweep::Exact`] is reachable from the public API; the other two are
/// the mutation-test variants (they must fail the FD check).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatrixSweep {
    /// `φ_iᵀ (∂K/∂X) φ_j` — the correct explicit term.
    Exact,
    /// Mutation (a): the explicit sweep zeroed (gradient identically 0).
    #[cfg(test)]
    DropExplicit,
    /// Mutation (b): `φ_i` passed where `φ_j` belongs (`φ_iᵀ ∂K φ_i`).
    #[cfg(test)]
    PhiIForPhiJ,
}

impl MatrixSweep {
    /// Whether the explicit sweep runs at all.
    fn runs(self) -> bool {
        match self {
            Self::Exact => true,
            #[cfg(test)]
            Self::DropExplicit => false,
            #[cfg(test)]
            Self::PhiIForPhiJ => true,
        }
    }

    /// Whether `φ_i` is (wrongly) used in place of `φ_j`.
    fn swaps_phi(self) -> bool {
        match self {
            Self::Exact => false,
            #[cfg(test)]
            Self::DropExplicit => false,
            #[cfg(test)]
            Self::PhiIForPhiJ => true,
        }
    }
}

/// Which pinned set a node belongs to in a [`CapacitanceMatrixShapeGradient`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinnedOwner {
    /// A free (unpinned) node.
    Free,
    /// A node of conductor `i` (index into the `conductors` slice).
    Conductor(usize),
    /// A node of the ground set.
    Ground,
}

/// Result of [`capacitance_matrix_shape_gradient`]: the `N × N` Maxwell
/// capacitance matrix of `N` conductors (plus ground) and the nodal shape
/// gradient of **every** entry, from one assembly, one LU factorization and
/// one `N`-column back-substitution.
#[derive(Debug, Clone)]
pub struct CapacitanceMatrixShapeGradient {
    /// Conductor names, in matrix order (the `conductors` order).
    pub names: Vec<String>,
    /// The Maxwell matrix `C_ij = φ_iᵀ K φ_j` (F), symmetric by construction.
    pub c: Vec<Vec<f64>>,
    /// Unit-excitation potentials `φ_i` (full length, Dirichlet values in
    /// place), one per conductor.
    pub phis: Vec<Vec<f64>>,
    /// Nodal reaction charges `K_full φ_i` (C per volt), one full-length
    /// vector per excitation. Summed over a node region they give that
    /// region's partial charge; see
    /// [`CapacitanceMatrixShapeGradient::reaction_partition`].
    pub reaction: Vec<Vec<f64>>,
    /// Pinned-set membership of every node (length `n_nodes`).
    pub owner: Vec<PinnedOwner>,
    /// Packed upper triangle of nodal gradients: entry `(i, j)` with
    /// `i ≤ j` lives at `i·N − i(i−1)/2 + (j − i)`.
    grad_pairs: Vec<Vec<[f64; 3]>>,
    /// LU factorizations performed — always `1`.
    pub n_factorizations: usize,
}

impl CapacitanceMatrixShapeGradient {
    /// Number of conductors `N`.
    pub fn n_conductors(&self) -> usize {
        self.names.len()
    }

    /// Matrix index of the conductor named `name`.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    fn pair_index(&self, i: usize, j: usize) -> usize {
        let n = self.n_conductors();
        assert!(
            i < n && j < n,
            "conductor index ({i}, {j}) out of range (N = {n})"
        );
        let (a, b) = if i <= j { (i, j) } else { (j, i) };
        // Row a of the packed upper triangle starts at Σ_{r<a} (N − r).
        a * n - a * a.saturating_sub(1) / 2 + (b - a)
    }

    /// Nodal gradient `∂C_ij/∂X_{n,d}` (F/m), one `[x, y, z]` triple per node.
    /// Symmetric: `grad_node(i, j)` and `grad_node(j, i)` are the same slice.
    ///
    /// # Panics
    ///
    /// Panics if `i` or `j` is out of range.
    pub fn grad_node(&self, i: usize, j: usize) -> &[[f64; 3]] {
        &self.grad_pairs[self.pair_index(i, j)]
    }

    /// `∂C_ij/∂θ` for a node-motion field `dnode_dtheta` (see
    /// [`chain_node_motion`]).
    pub fn dc_dtheta(&self, i: usize, j: usize, dnode_dtheta: &[[f64; 3]]) -> f64 {
        chain_node_motion(self.grad_node(i, j), dnode_dtheta)
    }

    /// The **floating-conductor reduction** of conductor `i` and its exact
    /// nodal gradient.
    ///
    /// The conductors in `floating` carry no net charge, so eliminating them
    /// (a Schur complement) gives the effective self-capacitance
    ///
    /// ```text
    ///   C_Σ = C_ii − C_iF C_FF⁻¹ C_Fi,
    /// ```
    ///
    /// which for one floating feedline `f` is `C_ii − C_if² / C_ff` (the
    /// transmon `C_Σ` of the #594 pipeline). With `w = C_FF⁻¹ C_Fi` the chain
    /// rule gives
    ///
    /// ```text
    ///   dC_Σ = dC_ii − 2 Σ_f w_f dC_if + Σ_{f,g} w_f w_g dC_fg,
    /// ```
    ///
    /// i.e. `dC_ii − 2 (C_if/C_ff) dC_if + (C_if/C_ff)² dC_ff` for one feedline.
    /// An empty `floating` returns `C_ii` and its gradient unchanged.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if an index is out of range, `i`
    /// is listed as floating, `floating` repeats an index, or `C_FF` is
    /// singular.
    pub fn floating_reduction(
        &self,
        i: usize,
        floating: &[usize],
    ) -> Result<FloatingReduction, ElectrostaticError> {
        let n = self.n_conductors();
        if i >= n || floating.iter().any(|&f| f >= n) {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "floating_reduction: index out of range (N = {n})"
            )));
        }
        if floating.contains(&i) {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "floating_reduction: conductor {i} cannot float against itself"
            )));
        }
        for (a, &f) in floating.iter().enumerate() {
            if floating[a + 1..].contains(&f) {
                return Err(ElectrostaticError::ShapeMismatch(format!(
                    "floating_reduction: floating conductor {f} listed twice"
                )));
            }
        }
        let m = floating.len();
        // w = C_FF⁻¹ C_Fi by Gaussian elimination with partial pivoting (m is
        // a handful of conductors).
        let mut a: Vec<Vec<f64>> = (0..m)
            .map(|r| {
                let mut row: Vec<f64> = floating.iter().map(|&g| self.c[floating[r]][g]).collect();
                row.push(self.c[floating[r]][i]);
                row
            })
            .collect();
        for col in 0..m {
            let piv = (col..m)
                .max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs()))
                .unwrap_or(col);
            if a[piv][col] == 0.0 || !a[piv][col].is_finite() {
                return Err(ElectrostaticError::ShapeMismatch(
                    "floating_reduction: C_FF is singular".to_string(),
                ));
            }
            a.swap(col, piv);
            for r in 0..m {
                if r != col {
                    let factor = a[r][col] / a[col][col];
                    let pivot_row = a[col].clone();
                    for (x, pv) in a[r].iter_mut().zip(&pivot_row).skip(col) {
                        *x -= factor * pv;
                    }
                }
            }
        }
        let w: Vec<f64> = (0..m).map(|r| a[r][m] / a[r][r]).collect();

        let mut c_sigma = self.c[i][i];
        for (k, &f) in floating.iter().enumerate() {
            c_sigma -= self.c[i][f] * w[k];
        }
        let n_nodes = self.owner.len();
        let mut grad = self.grad_node(i, i).to_vec();
        debug_assert_eq!(grad.len(), n_nodes);
        for (k, &f) in floating.iter().enumerate() {
            let gif = self.grad_node(i, f);
            for (acc, g) in grad.iter_mut().zip(gif) {
                for d in 0..3 {
                    acc[d] -= 2.0 * w[k] * g[d];
                }
            }
            for (l, &g_idx) in floating.iter().enumerate() {
                let gfg = self.grad_node(f, g_idx);
                let wk_wl = w[k] * w[l];
                for (acc, g) in grad.iter_mut().zip(gfg) {
                    for d in 0..3 {
                        acc[d] += wk_wl * g[d];
                    }
                }
            }
        }
        Ok(FloatingReduction {
            c_sigma,
            weights: w,
            grad_node: grad,
        })
    }

    /// **Partial-charge partition** of excitation `i`: the reaction charge
    /// `(K φ_i)_n` summed over each named node region.
    ///
    /// With conductor `i` at 1 V and every other pinned node at 0 V,
    /// `C_ii = Σ_{n ∈ i} (K φ_i)_n` (plus a free-row round-off term), and the
    /// columns of the full `K` sum to zero (constants are in its kernel), so
    /// the charges on the other pinned nodes sum to `−C_ii` up to the
    /// free-row residual. The `regions` must be a **disjoint, exhaustive**
    /// partition of the pinned nodes that are NOT conductor `i` (ground plus
    /// the other conductors): no node in two regions, no conductor-`i` or
    /// free node in any region, and every such pinned node in exactly one.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if `i` is out of range or the
    /// regions are not a disjoint, exhaustive partition as above.
    pub fn reaction_partition(
        &self,
        i: usize,
        regions: &[(String, Vec<u32>)],
    ) -> Result<ReactionPartition, ElectrostaticError> {
        let n = self.n_conductors();
        if i >= n {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "reaction_partition: conductor {i} out of range (N = {n})"
            )));
        }
        let n_nodes = self.owner.len();
        let mut region_of: Vec<Option<usize>> = vec![None; n_nodes];
        for (r, (name, nodes)) in regions.iter().enumerate() {
            for &g in nodes {
                let gu = g as usize;
                if gu >= n_nodes {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "reaction_partition: region {name:?} node {g} out of range"
                    )));
                }
                match self.owner[gu] {
                    PinnedOwner::Free => {
                        return Err(ElectrostaticError::ShapeMismatch(format!(
                            "reaction_partition: region {name:?} holds free node {g}"
                        )));
                    }
                    PinnedOwner::Conductor(k) if k == i => {
                        return Err(ElectrostaticError::ShapeMismatch(format!(
                            "reaction_partition: region {name:?} holds node {g} of the excited \
                             conductor {i}"
                        )));
                    }
                    _ => {}
                }
                if let Some(prev) = region_of[gu] {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "reaction_partition: node {g} in both {:?} and {name:?}",
                        regions[prev].0
                    )));
                }
                region_of[gu] = Some(r);
            }
        }
        let kphi = &self.reaction[i];
        let mut sums = vec![0.0_f64; regions.len()];
        let mut self_charge = 0.0_f64;
        let mut free_residual = 0.0_f64;
        for g in 0..n_nodes {
            match (self.owner[g], region_of[g]) {
                (PinnedOwner::Free, _) => free_residual += kphi[g],
                (PinnedOwner::Conductor(k), _) if k == i => self_charge += kphi[g],
                (_, Some(r)) => sums[r] += kphi[g],
                (_, None) => {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "reaction_partition: pinned node {g} is in no region (the partition \
                         must be exhaustive)"
                    )));
                }
            }
        }
        Ok(ReactionPartition {
            self_charge,
            regions: regions
                .iter()
                .zip(sums)
                .map(|((name, nodes), q)| (name.clone(), nodes.len(), q))
                .collect(),
            free_residual,
        })
    }
}

/// The floating-conductor reduction `C_Σ` of one conductor and its nodal
/// gradient (see [`CapacitanceMatrixShapeGradient::floating_reduction`]).
#[derive(Debug, Clone)]
pub struct FloatingReduction {
    /// `C_Σ = C_ii − C_iF C_FF⁻¹ C_Fi` (F).
    pub c_sigma: f64,
    /// `w = C_FF⁻¹ C_Fi`, one weight per floating conductor (`C_if/C_ff` for
    /// a single feedline).
    pub weights: Vec<f64>,
    /// Exact nodal gradient `∂C_Σ/∂X_{n,d}` (F/m).
    pub grad_node: Vec<[f64; 3]>,
}

impl FloatingReduction {
    /// `∂C_Σ/∂θ` for a node-motion field (see [`chain_node_motion`]).
    pub fn dc_dtheta(&self, dnode_dtheta: &[[f64; 3]]) -> f64 {
        chain_node_motion(&self.grad_node, dnode_dtheta)
    }
}

/// Partial-charge partition of one unit excitation (see
/// [`CapacitanceMatrixShapeGradient::reaction_partition`]).
#[derive(Debug, Clone)]
pub struct ReactionPartition {
    /// `Σ (K φ_i)` over the excited conductor's own nodes (≈ `C_ii`).
    pub self_charge: f64,
    /// `(name, node count, Σ (K φ_i) over the region)`, in the given order.
    pub regions: Vec<(String, usize, f64)>,
    /// `Σ (K φ_i)` over the free nodes — round-off, since the free rows are
    /// the solved equilibrium.
    pub free_residual: f64,
}

impl ReactionPartition {
    /// Sum of the region charges (≈ `−C_ii`).
    pub fn regions_total(&self) -> f64 {
        self.regions.iter().map(|r| r.2).sum()
    }
}

/// The **full Maxwell capacitance matrix** of `conductors` (plus `ground`)
/// and the exact nodal shape gradient of every entry, from **one** assembly,
/// **one** LU factorization and one `N`-column back-substitution.
///
/// Excitation `i` pins conductor `i` at 1 V and every other conductor and
/// the ground at 0 V, exactly as
/// [`crate::assembly::electrostatic::extract_capacitance`] does; the
/// Dirichlet set is the same for every excitation, so the reduced `K_ff` is
/// shared and the `N` right-hand sides are solved against its single
/// factorization. Each entry is `C_ij = φ_iᵀ K φ_j` and its gradient is the
/// explicit term `φ_iᵀ (∂K/∂X) φ_j` alone (see the section comment above:
/// both implicit terms vanish because `φ_i`, `φ_j` are discrete-harmonic),
/// evaluated with the exact dual-number element kernel.
///
/// The conductors are **arbitrary Dirichlet regions**: any node sets, so a
/// caller can split a physical conductor into several electrodes (for
/// instance a claw region carved out of a ground plane) as long as the sets
/// are disjoint from each other and from `ground`. Their `voltage` fields are
/// ignored (unit excitations are imposed).
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] if `conductors` is empty, a node is
/// out of range, or a node belongs to two conductors or to a conductor and
/// `ground`; otherwise propagates assembly / factorization errors.
pub fn capacitance_matrix_shape_gradient(
    mesh: &TetMesh,
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
) -> Result<CapacitanceMatrixShapeGradient, ElectrostaticError> {
    capacitance_matrix_sweep(mesh, eps_r, conductors, ground, MatrixSweep::Exact)
}

fn capacitance_matrix_sweep(
    mesh: &TetMesh,
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
    sweep: MatrixSweep,
) -> Result<CapacitanceMatrixShapeGradient, ElectrostaticError> {
    let n = conductors.len();
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if n == 0 {
        return Err(ElectrostaticError::ShapeMismatch(
            "capacitance_matrix_shape_gradient: at least one conductor required".to_string(),
        ));
    }

    // Pinned-set membership; reject overlapping Dirichlet regions.
    let mut owner = vec![PinnedOwner::Free; n_nodes];
    for &g in ground {
        let gu = g as usize;
        if gu >= n_nodes {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "ground node {g} out of range (n_nodes {n_nodes})"
            )));
        }
        owner[gu] = PinnedOwner::Ground;
    }
    for (k, c) in conductors.iter().enumerate() {
        for &g in &c.nodes {
            let gu = g as usize;
            if gu >= n_nodes {
                return Err(ElectrostaticError::ShapeMismatch(format!(
                    "conductor {:?} node {g} out of range (n_nodes {n_nodes})",
                    c.name
                )));
            }
            match owner[gu] {
                PinnedOwner::Free => owner[gu] = PinnedOwner::Conductor(k),
                PinnedOwner::Conductor(k2) if k2 == k => {}
                other => {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "conductor {:?} node {g} is already pinned as {other:?}; Dirichlet \
                         regions must be disjoint",
                        c.name
                    )));
                }
            }
        }
    }

    // --- Assemble once, factor once. ---
    let rho = vec![0.0_f64; n_tets];
    let sys = assemble_electrostatic(mesh, eps_r, &rho, conductors, ground)?;
    let lu = sys
        .k
        .as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // --- Fold the pinned columns of the full K into an n_free × N RHS:
    //     b[fi, e] = −Σ_{j ∈ conductor e} K_full[i, j] · 1. ---
    let mut rhs: Mat<f64> = Mat::zeros(sys.n_free, n);
    {
        let k_ref = sys.k_full.as_ref();
        let cp = k_ref.col_ptr();
        let row_idx = k_ref.row_idx();
        let vals = k_ref.val();
        for j in 0..n_nodes {
            let PinnedOwner::Conductor(e) = owner[j] else {
                continue; // free column (LHS) or ground (0 V: no fold-in)
            };
            for idx in cp[j]..cp[j + 1] {
                if let Some(fi) = sys.free_of_global[row_idx[idx]] {
                    rhs[(fi, e)] -= vals[idx];
                }
            }
        }
    }
    lu.solve_in_place(rhs.as_mut());

    let mut phis: Vec<Vec<f64>> = Vec::with_capacity(n);
    for e in 0..n {
        let mut phi = vec![0.0_f64; n_nodes];
        for g in 0..n_nodes {
            phi[g] = match (owner[g], sys.free_of_global[g]) {
                (PinnedOwner::Conductor(k), _) => f64::from(u8::from(k == e)),
                (PinnedOwner::Ground, _) => 0.0,
                (PinnedOwner::Free, Some(fi)) => rhs[(fi, e)],
                (PinnedOwner::Free, None) => 0.0,
            };
        }
        phis.push(phi);
    }

    let reaction: Vec<Vec<f64>> = phis.iter().map(|p| kfull_matvec(&sys.k_full, p)).collect();
    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v: f64 = phis[j].iter().zip(&reaction[i]).map(|(a, b)| a * b).sum();
            c[i][j] = v;
            c[j][i] = v;
        }
    }

    // --- Geometry sweep: ∂C_ij/∂X = Σ_t ε₀ε_r ∂/∂X (φ_iᵀ K_local φ_j). ---
    let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i..n).map(move |j| (i, j))).collect();
    let mut grad_pairs = vec![vec![[0.0_f64; 3]; n_nodes]; pairs.len()];
    if sweep.runs() {
        let mut loc = vec![[0.0_f64; 4]; n];
        for (t, tet) in mesh.tets.iter().enumerate() {
            let base = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            for (e, l) in loc.iter_mut().enumerate() {
                *l = [
                    phis[e][tet[0] as usize],
                    phis[e][tet[1] as usize],
                    phis[e][tet[2] as usize],
                    phis[e][tet[3] as usize],
                ];
            }
            let eps_t = EPS_0 * eps_r[t];
            for a in 0..4 {
                let node = tet[a] as usize;
                for d in 0..3 {
                    let mut dc = base.map(|v| v.map(Dual::cst));
                    dc[a][d] = Dual::var(base[a][d]);
                    for (p, &(i, j)) in pairs.iter().enumerate() {
                        let right = if sweep.swaps_phi() { &loc[i] } else { &loc[j] };
                        let du = stiffness_bilinear_dual(&dc, &loc[i], right).du;
                        grad_pairs[p][node][d] += eps_t * du;
                    }
                }
            }
        }
    }

    Ok(CapacitanceMatrixShapeGradient {
        names: conductors.iter().map(|c| c.name.clone()).collect(),
        c,
        phis,
        reaction,
        owner,
        grad_pairs,
        n_factorizations,
    })
}

#[cfg(test)]
mod tests;
