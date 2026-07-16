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

use crate::assembly::electrostatic::{
    EPS_0, Electrode, ElectrostaticError, assemble_electrostatic,
};
use crate::mesh::TetMesh;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::electrostatic::{assemble_electrostatic, tet_p1_local};
    use crate::mesh::cube_tet_mesh;

    /// Objective `g(φ) = ½ Σ_i φ_i²` and its cotangent `∂g/∂φ = φ`.
    fn quadratic_objective(phi: &[f64]) -> (f64, Vec<f64>) {
        let g = 0.5 * phi.iter().map(|p| p * p).sum::<f64>();
        (g, phi.to_vec())
    }

    /// Unit-cube parallel-plate capacitor: hi face (x=1) at 1 V, lo face
    /// (x=0) grounded, uniform ε_r. Returns `(mesh, eps_r, electrodes,
    /// ground)`.
    fn capacitor_fixture(n: usize) -> (TetMesh, Vec<f64>, Vec<Electrode>, Vec<u32>) {
        let mesh = cube_tet_mesh(n, 1.0);
        let eps_r = vec![3.0_f64; mesh.n_tets()];
        let tol = 1e-9;
        let hi: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let lo: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0].abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let electrodes = vec![Electrode {
            name: "hi".into(),
            nodes: hi,
            voltage: 1.0,
        }];
        (mesh, eps_r, electrodes, lo)
    }

    /// The real `f64` element-stiffness bilinear form `λᵀ K_local φ`, used to
    /// cross-check the dual derivative by central finite difference.
    fn kernel_bilinear(coords: &[[f64; 3]; 4], lam: &[f64; 4], phi: &[f64; 4]) -> f64 {
        let (k, _m, _v) = tet_p1_local(coords);
        let mut s = 0.0;
        for p in 0..4 {
            for q in 0..4 {
                s += lam[p] * k[p][q] * phi[q];
            }
        }
        s
    }

    /// **Element-kernel derivative is exact.** The dual-number tangent of
    /// the P1 stiffness bilinear form must match a central finite difference
    /// of the real `f64` kernel for every one of the twelve node
    /// coordinates, to tight tolerance — proving `∂K_local/∂X` is analytic
    /// (forward-mode AD), not an FD approximation. A sign flip or a dropped
    /// term in [`stiffness_bilinear_dual`] fails this immediately.
    #[test]
    fn dual_element_derivative_matches_kernel_finite_difference() {
        // A generic, well-shaped (non-axis-aligned) tet so every coordinate
        // has a distinct nonzero sensitivity.
        let base = [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ];
        let lam = [0.7, -1.3, 0.4, 1.1];
        let phi = [0.2, 0.9, -0.5, 1.4];

        let h = 1e-6;
        let mut worst = 0.0_f64;
        for a in 0..4 {
            for c in 0..3 {
                // Analytic (dual) tangent.
                let mut dc = [[Dual::cst(0.0); 3]; 4];
                for (av, dv) in dc.iter_mut().enumerate() {
                    for (cv, comp) in dv.iter_mut().enumerate() {
                        *comp = if av == a && cv == c {
                            Dual::var(base[av][cv])
                        } else {
                            Dual::cst(base[av][cv])
                        };
                    }
                }
                let ana = stiffness_bilinear_dual(&dc, &lam, &phi).du;

                // Central FD of the real kernel.
                let mut cp = base;
                let mut cm = base;
                cp[a][c] += h;
                cm[a][c] -= h;
                let fd = (kernel_bilinear(&cp, &lam, &phi) - kernel_bilinear(&cm, &lam, &phi))
                    / (2.0 * h);

                let rel = (ana - fd).abs() / fd.abs().max(1e-12);
                worst = worst.max(rel);
                assert!(
                    rel < 1e-6,
                    "vertex {a} axis {c}: dual {ana} vs FD {fd}, rel-err {rel:.3e}"
                );
                // Guard against a degenerate all-zero derivative masking a bug.
                assert!(fd.abs() > 1e-9, "vertex {a} axis {c} derivative ~0");
            }
        }
        assert!(worst < 1e-6, "worst dual-vs-FD rel-err {worst:.3e}");
    }

    /// The `.re` field of the dual bilinear form reproduces the real `f64`
    /// element bilinear form exactly (the dual pass is a faithful lift of
    /// [`tet_p1_local`]).
    #[test]
    fn dual_bilinear_reproduces_real_value() {
        let base = [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ];
        let lam = [0.7, -1.3, 0.4, 1.1];
        let phi = [0.2, 0.9, -0.5, 1.4];
        let dc = base.map(|v| v.map(Dual::cst));
        let dual = stiffness_bilinear_dual(&dc, &lam, &phi).re;
        let real = kernel_bilinear(&base, &lam, &phi);
        assert!(
            (dual - real).abs() <= 1e-12 * real.abs().max(1e-12),
            "dual .re {dual} != real {real}"
        );
    }

    /// **The load-bearing test.** The discrete-adjoint **shape** gradient
    /// `∂g/∂θ` — one forward + one adjoint solve + the geometry Jacobian —
    /// must match a full central finite difference of the entire pipeline
    /// (perturb θ → **move the nodes** → re-assemble K on the moved mesh →
    /// re-solve → recompute g), for two distinct node-motion maps, to a
    /// tight relative tolerance. This proves the shape gradient is
    /// *correct*, not merely that it runs; a wrong sign, a wrong `∂K/∂node`,
    /// or a broken θ-chain fails it.
    #[test]
    fn shape_gradient_matches_central_finite_difference() {
        let (mesh, eps_r, electrodes, ground) = capacitor_fixture(4);

        // ONE forward + ONE adjoint solve → full nodal-coordinate gradient.
        let sg =
            electrostatic_shape_gradient(&mesh, &eps_r, &electrodes, &ground, quadratic_objective)
                .unwrap();
        assert_eq!(
            sg.n_factorizations, 1,
            "shape adjoint must reuse the forward factorization (no refactorize)"
        );

        // Two analytic node-motion maps, both LINEAR in θ so X(θ)=X⁰+θ·D and
        // the constant velocity field D is exact. Both genuinely *distort*
        // the field (a *uniform* affine scale/shift would map the linear
        // capacitor solution φ=x to itself at the nodes — a physically null
        // gradient — so we deliberately use non-uniform morphs).
        //
        //  1. Translate ONLY the hi electrode face (x=1) in +x, keeping the
        //     interior fixed: D_n=[1,0,0] on the face, else 0. Stretches just
        //     the last tet layer, so the gap conductance changes and φ shifts.
        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        //  2. Move a single interior control node in +x: D_n=[1,0,0] for the
        //     interior node nearest the domain centre, else 0. A localized
        //     one-node morph — the sharpest "distinct nonzero" probe.
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                // interior only (not on any face), else its coordinate may be
                // pinned/boundary and the morph would leave the box.
                p.iter().all(|&c| c > tol && c < 1.0 - tol)
            })
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        // Full-pipeline objective as a function of θ under a given velocity
        // field D: move nodes to X⁰+θD, re-assemble, re-solve, recompute g.
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let rho = vec![0.0; moved.n_tets()];
            let sys = assemble_electrostatic(&moved, &eps_r, &rho, &electrodes, &ground).unwrap();
            let phi = sys.solve().unwrap();
            quadratic_objective(&phi).0
        };

        let h = 1e-6;
        for (name, d) in [
            ("hi-face-translate", &d_face),
            ("interior-control-node", &d_node),
        ] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            // Observed rel-err ~7e-12 (hi-face) / ~1e-9 (control-node): the
            // adjoint gradient is AD-exact, so only the full-pipeline FD's own
            // O(h²) truncation + solver round-off remain — orders below 1e-4.
            // Each map must exercise a genuinely nonzero shape sensitivity.
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            assert!(
                rel < 1e-4,
                "map {name}: adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-4"
            );
        }

        // The two maps must give DISTINCT gradients (they probe different
        // geometry perturbations), else the test could pass on a constant.
        let g_face = chain_node_motion(&sg.grad_node, &d_face);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_face - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_face} vs {g_node})"
        );
    }
}
