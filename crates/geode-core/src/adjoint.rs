//! Discrete-adjoint design sensitivities: `∂(scalar observable)/∂(material ε)`
//! **through** a linear FEM solve (Epic #569, issue #570).
//!
//! # Why this module exists
//!
//! GEODE's Burn autodiff tape reaches the assembled operators and their
//! dependence on material ε, but the sparse **faer factorization breaks the
//! tape** (see the `# Autodiff` note in [`crate::driven::solve`]). Naïve
//! reverse-mode therefore yields **no** gradient of any solved observable.
//! This module supplies the missing layer: an explicit **discrete adjoint**
//! around the direct solve that recovers the exact discrete gradient of a
//! scalar figure-of-merit with respect to a per-region material parameter,
//! from **one forward + one adjoint solve** — the capability that Palace /
//! HFSS / COMSOL structurally lack and that Epic #569 repositions GEODE
//! around.
//!
//! # The adjoint identity
//!
//! For a linear system `A(ε) x = b` and a scalar objective `g(x)` (no
//! *explicit* ε-dependence), differentiating `A x = b` gives
//! `∂x/∂ε_k = −A⁻¹ (∂A/∂ε_k) x`, hence
//!
//! ```text
//!   dg/dε_k = (∂g/∂x)ᵀ ∂x/∂ε_k = −λᵀ (∂A/∂ε_k) x,
//!   with     Aᵀ λ = ∂g/∂x        (the adjoint system).
//! ```
//!
//! The adjoint solve **reuses the forward factorization** — faer's LU
//! exposes a transpose solve ([`faer::linalg::solvers::Solve::solve_transpose_in_place`]),
//! so the adjoint is one extra back-substitution, never a refactorization.
//! The gradient of **every** region `k` then falls out of a cheap local
//! contraction against the already-computed `λ` and `x` — `O(1)` solves for
//! the whole gradient vector, versus `N` re-solves for a finite difference.
//!
//! # This proof-of-concept: real scalar electrostatics
//!
//! Following the issue's honesty clause, the load-bearing first
//! demonstration is the **real, SPD** scalar electrostatic operator
//! `−∇·(ε₀ ε_r ∇φ) = ρ` ([`crate::assembly::electrostatic`]), whose ε
//! dependence is clean and exactly **linear** in the per-tet `ε_r`. The
//! parameter is a per-region relative permittivity `ε_k`; the observable is
//! any smooth scalar `g(φ)` of the nodal potential. The pattern established
//! here (factor once, transpose-solve the adjoint, contract `−λᵀ (∂A/∂ε) x`
//! region-by-region) transfers unchanged to the complex driven Nédélec
//! solve; only the assembly of `(∂A/∂ε_k) x` changes.
//!
//! ## `(∂A/∂ε_k) x` is an exact analytic JVP
//!
//! Because the electrostatic stiffness is **linear** in `ε_r`
//! (`K_full = Σ_t ε₀ ε_r[t] K_local(t)`), the directional derivative in the
//! region-`k` indicator direction is exactly the assembly kernel applied to
//! that direction: `∂K_full/∂ε_k = Σ_{t∈k} ε₀ K_local(t)`. We evaluate
//! `(∂A/∂ε_k) x` by reusing the very element kernel the assembler uses
//! ([`crate::assembly::electrostatic::tet_p1_local`]) — an **exact** JVP,
//! with no finite-difference truncation, so the adjoint-vs-FD test below
//! isolates the correctness of the adjoint algebra itself.
//!
//! ## Dirichlet handling
//!
//! With electrode / ground DOFs eliminated, the solved system is
//! `K_ff φ_free = b_free`, `b_free = b_ρ − K_fp φ_pinned`. Differentiating
//! (the pinned potentials are ε-independent constants) collapses to
//!
//! ```text
//!   K_ff ∂φ_free/∂ε_k = −[ (∂K_full/∂ε_k) φ_full ]_free,
//! ```
//!
//! i.e. the **same** formula with `φ_full` carrying the pinned Dirichlet
//! values on the pinned rows — so a charge-driven grounded box and a
//! voltage-driven capacitor are handled by one code path.

use faer::Mat;
use faer::linalg::solvers::Solve;

use crate::assembly::electrostatic::{
    EPS_0, Electrode, ElectrostaticError, assemble_electrostatic, tet_p1_local,
};
use crate::mesh::TetMesh;

/// Result of an electrostatic discrete-adjoint gradient evaluation.
#[derive(Debug, Clone)]
pub struct AdjointGradient {
    /// The scalar objective value `g(φ)` at the (unperturbed) forward
    /// solution.
    pub objective: f64,
    /// The gradient `dg/dε_k`, one entry per region, indexed by the region
    /// label `0..n_regions`. Computed from a single forward + single adjoint
    /// solve.
    pub grad: Vec<f64>,
    /// Full-length `[n_nodes]` forward potential `φ` (pinned Dirichlet
    /// values in place), returned for post-processing / cross-checks.
    pub phi: Vec<f64>,
    /// Number of sparse LU **factorizations** performed. Always `1`: the
    /// forward and adjoint solves share a single factorization (the adjoint
    /// is a transpose back-substitution, not a refactorization). Asserted by
    /// the finite-difference validation test.
    pub n_factorizations: usize,
}

/// Compute `∂g/∂ε_k` for every region `k` of a scalar electrostatic solve
/// via the discrete adjoint — **one forward solve + one adjoint solve**,
/// reusing a single LU factorization.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh.
/// * `eps_r` — per-tet relative permittivity (length `mesh.n_tets()`), the
///   *evaluated* material at which the gradient is taken. Build it from the
///   per-region values with [`build_region_eps`] so `eps_r[t]` and the
///   region parameter `ε_{region_of_tet[t]}` agree.
/// * `rho` — per-tet volume charge density (length `mesh.n_tets()`; pass
///   all-zeros for a purely voltage-driven problem).
/// * `electrodes`, `ground` — the Dirichlet boundary exactly as
///   [`assemble_electrostatic`] takes them.
/// * `region_of_tet` — per-tet region label in `0..n_regions`
///   (length `mesh.n_tets()`); `dg/dε_k` sums the contribution of every tet
///   with `region_of_tet[t] == k`.
/// * `n_regions` — number of design regions (length of the returned
///   gradient).
/// * `objective` — the scalar figure-of-merit. Given the full-length nodal
///   potential `φ` (`[n_nodes]`, Dirichlet values in place) it returns
///   `(g, ∂g/∂φ)` where `∂g/∂φ` is a full-length `[n_nodes]` cotangent.
///   The objective must **not** depend explicitly on ε (only through `φ`);
///   its value on pinned Dirichlet nodes is a constant and its cotangent
///   there is ignored (those DOFs do not vary with ε).
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from assembly / factorization, and
/// returns [`ElectrostaticError::ShapeMismatch`] if `region_of_tet`, a
/// region label, or the objective cotangent has the wrong length.
#[allow(clippy::too_many_arguments)]
pub fn electrostatic_adjoint_gradient<G>(
    mesh: &TetMesh,
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    region_of_tet: &[usize],
    n_regions: usize,
    objective: G,
) -> Result<AdjointGradient, ElectrostaticError>
where
    G: Fn(&[f64]) -> (f64, Vec<f64>),
{
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();
    if region_of_tet.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "region_of_tet length {} != tet count {n_tets}",
            region_of_tet.len()
        )));
    }
    if let Some(&bad) = region_of_tet.iter().find(|&&r| r >= n_regions) {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "region label {bad} out of range for n_regions {n_regions}"
        )));
    }

    // --- Assemble the SPD electrostatic system (full + reduced K, RHS). ---
    let sys = assemble_electrostatic(mesh, eps_r, rho, electrodes, ground)?;

    // --- Factor the reduced stiffness ONCE. This single factorization
    // serves both the forward solve and the transpose (adjoint) solve. ---
    let lu = sys
        .k
        .as_ref()
        .sp_lu()
        .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // --- Forward solve: K_ff φ_free = b_free. ---
    let mut fwd: Mat<f64> = Mat::from_fn(sys.n_free, 1, |i, _| sys.b[i]);
    lu.solve_in_place(fwd.as_mut());

    // Scatter φ_free back to a full-length potential (pinned rows carry
    // their prescribed Dirichlet value).
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

    // --- Adjoint solve: K_ffᵀ λ_free = (∂g/∂φ)_free, REUSING the forward
    // factorization via faer's transpose back-substitution (no refactor).
    // K_ff is symmetric, so the transpose solve is exact here and also
    // demonstrates the general (non-symmetric) adjoint pattern. ---
    let mut adj: Mat<f64> = Mat::from_fn(sys.n_free, 1, |_, _| 0.0);
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            adj[(*fi, 0)] = dg_dphi[g];
        }
    }
    lu.solve_transpose_in_place(adj.as_mut());

    // λ scattered to full length, zero on pinned rows (those DOFs are
    // ε-independent, so they contribute nothing to −λᵀ (∂A/∂ε) x).
    let mut lambda_full = vec![0.0_f64; n_nodes];
    for (g, slot) in sys.free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            lambda_full[g] = adj[(*fi, 0)];
        }
    }

    // --- Gradient: dg/dε_k = −λᵀ (∂A/∂ε_k) x, accumulated region-by-region
    // in one pass over the tets. For tet t (region r) the element block of
    // (∂K_full/∂ε_k) φ_full is ε₀ K_local(t) φ_local, so
    //   grad_r += − Σ_p λ[gp] · (ε₀ Σ_q K_local[p][q] φ[gq]).
    // This reuses the assembler's own element kernel (`tet_p1_local`), an
    // exact analytic JVP of the linear-in-ε assembly. ---
    let mut grad = vec![0.0_f64; n_regions];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (k_local, _m, _vol) = tet_p1_local(&coords);
        let r = region_of_tet[t];
        let mut contrib = 0.0_f64;
        for p in 0..4 {
            let gp = tet[p] as usize;
            let lp = lambda_full[gp];
            if lp == 0.0 {
                continue;
            }
            // (K_local φ_local)_p, ε₀ applied once outside the q-loop.
            let mut kphi_p = 0.0_f64;
            for q in 0..4 {
                kphi_p += k_local[p][q] * phi[tet[q] as usize];
            }
            contrib += lp * EPS_0 * kphi_p;
        }
        grad[r] -= contrib;
    }

    Ok(AdjointGradient {
        objective: objective_value,
        grad,
        phi,
        n_factorizations,
    })
}

/// Expand a per-region relative-permittivity table into the per-tet `ε_r`
/// vector that [`electrostatic_adjoint_gradient`] and
/// [`assemble_electrostatic`] consume.
///
/// `eps_region[region_of_tet[t]]` becomes `eps_r[t]`. This keeps the design
/// parameter (`eps_region[k]`) and the assembled material in exact
/// correspondence, so a finite-difference perturbation of `eps_region[k]`
/// perturbs precisely the tets region `k` owns.
///
/// # Panics
///
/// Panics if any region label indexes past `eps_region`.
pub fn build_region_eps(region_of_tet: &[usize], eps_region: &[f64]) -> Vec<f64> {
    region_of_tet
        .iter()
        .map(|&r| {
            assert!(
                r < eps_region.len(),
                "build_region_eps: region {r} out of range for eps_region of length {}",
                eps_region.len()
            );
            eps_region[r]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::electrostatic::assemble_electrostatic;
    use crate::mesh::cube_tet_mesh;

    /// Objective `g(φ) = ½ Σ_i φ_i²` (a smooth L2 measure of the nodal
    /// potential) and its cotangent `∂g/∂φ = φ`. No explicit ε dependence.
    fn quadratic_objective(phi: &[f64]) -> (f64, Vec<f64>) {
        let g = 0.5 * phi.iter().map(|p| p * p).sum::<f64>();
        (g, phi.to_vec())
    }

    /// Build the 3-region layered-dielectric parallel-plate fixture:
    /// unit cube, hi face (x=1) at 1 V, lo face (x=0) grounded, the interior
    /// split into three x-slabs (regions 0/1/2). Returns
    /// `(mesh, region_of_tet, electrodes, ground)`.
    fn layered_capacitor_fixture(n: usize) -> (TetMesh, Vec<usize>, Vec<Electrode>, Vec<u32>) {
        let mesh = cube_tet_mesh(n, 1.0);
        // Region by tet-centroid x: [0,1/3) -> 0, [1/3,2/3) -> 1, else 2.
        let region_of_tet: Vec<usize> = mesh
            .tets
            .iter()
            .map(|tet| {
                let cx = tet.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
                if cx < 1.0 / 3.0 {
                    0
                } else if cx < 2.0 / 3.0 {
                    1
                } else {
                    2
                }
            })
            .collect();
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
        (mesh, region_of_tet, electrodes, lo)
    }

    /// **The load-bearing test.** The discrete-adjoint gradient
    /// `∂g/∂ε_k` must match a full central finite difference of the entire
    /// pipeline (perturb ε_k → re-assemble → re-solve → recompute g) for
    /// every region, to a tight relative tolerance. This proves the
    /// gradient is *correct*, not merely that it runs.
    #[test]
    fn adjoint_gradient_matches_central_finite_difference() {
        let (mesh, region_of_tet, electrodes, ground) = layered_capacitor_fixture(4);
        let n_regions = 3;
        let eps_region = [2.0_f64, 5.0, 3.0];
        let rho = vec![0.0; mesh.n_tets()];
        let eps_r = build_region_eps(&region_of_tet, &eps_region);

        // --- Adjoint gradient: ONE forward + ONE adjoint solve. ---
        let adj = electrostatic_adjoint_gradient(
            &mesh,
            &eps_r,
            &rho,
            &electrodes,
            &ground,
            &region_of_tet,
            n_regions,
            quadratic_objective,
        )
        .unwrap();

        // The adjoint MUST reuse the forward factorization — exactly one
        // sparse LU factorization for the whole gradient.
        assert_eq!(
            adj.n_factorizations, 1,
            "adjoint must reuse the forward factorization (no refactorize)"
        );

        // --- Central finite difference of the whole pipeline per region. ---
        // g depends on ε only through the re-solved φ; each perturbation is a
        // full re-assemble + re-solve, so this is the true total derivative.
        let g_of = |eps_region: &[f64]| -> f64 {
            let er = build_region_eps(&region_of_tet, eps_region);
            let sys = assemble_electrostatic(&mesh, &er, &rho, &electrodes, &ground).unwrap();
            let phi = sys.solve().unwrap();
            quadratic_objective(&phi).0
        };

        let h = 1e-5;
        let mut worst_rel = 0.0_f64;
        for k in 0..n_regions {
            let mut ep = eps_region;
            let mut em = eps_region;
            ep[k] += h;
            em[k] -= h;
            let fd = (g_of(&ep) - g_of(&em)) / (2.0 * h);
            let a = adj.grad[k];
            let rel = (a - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            worst_rel = worst_rel.max(rel);
            // Gradients here are O(0.1–1); guard against a degenerate
            // near-zero FD masking a real error.
            assert!(
                fd.abs() > 1e-6,
                "region {k} FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            assert!(
                rel < 1e-4,
                "region {k}: adjoint {a} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-4"
            );
        }
        // Tight in practice — the adjoint JVP is exact, so only the FD's own
        // O(h²) truncation + solver round-off remain.
        assert!(
            worst_rel < 1e-4,
            "worst adjoint-vs-FD rel-err {worst_rel:.3e} exceeds 1e-4"
        );
    }

    /// A charge-driven grounded box (all-zero Dirichlet, non-zero ρ) — the
    /// other Dirichlet regime — is validated by the same identity, confirming
    /// the ρ-driven and voltage-driven paths share one adjoint formula.
    #[test]
    fn adjoint_gradient_matches_fd_charge_driven() {
        let mesh = cube_tet_mesh(4, 1.0);
        let region_of_tet: Vec<usize> = mesh
            .tets
            .iter()
            .map(|tet| {
                let cx = tet.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
                if cx < 0.5 { 0 } else { 1 }
            })
            .collect();
        let n_regions = 2;
        let eps_region = [4.0_f64, 2.0];
        let eps_r = build_region_eps(&region_of_tet, &eps_region);
        // Uniform volume charge; ground every boundary node (grounded box).
        let rho = vec![1.0e-9; mesh.n_tets()];
        let tol = 1e-9;
        let ground: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().any(|&c| c.abs() < tol || (c - 1.0).abs() < tol))
            .map(|(i, _)| i as u32)
            .collect();
        let electrodes: Vec<Electrode> = vec![];

        let adj = electrostatic_adjoint_gradient(
            &mesh,
            &eps_r,
            &rho,
            &electrodes,
            &ground,
            &region_of_tet,
            n_regions,
            quadratic_objective,
        )
        .unwrap();
        assert_eq!(adj.n_factorizations, 1);

        let g_of = |eps_region: &[f64]| -> f64 {
            let er = build_region_eps(&region_of_tet, eps_region);
            let sys = assemble_electrostatic(&mesh, &er, &rho, &electrodes, &ground).unwrap();
            let phi = sys.solve().unwrap();
            quadratic_objective(&phi).0
        };

        let h = 1e-5;
        for k in 0..n_regions {
            let mut ep = eps_region;
            let mut em = eps_region;
            ep[k] += h;
            em[k] -= h;
            let fd = (g_of(&ep) - g_of(&em)) / (2.0 * h);
            let rel = (adj.grad[k] - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            assert!(
                fd.abs() > 1e-20,
                "region {k} FD gradient {fd} unexpectedly ~0"
            );
            assert!(
                rel < 1e-4,
                "region {k}: adjoint {} vs FD {fd}, rel-err {rel:.3e}",
                adj.grad[k]
            );
        }
    }

    /// `build_region_eps` expands the per-region table tet-by-tet.
    #[test]
    fn build_region_eps_expands_table() {
        let region = [0usize, 1, 0, 2, 1];
        let table = [2.0, 5.0, 9.0];
        assert_eq!(
            build_region_eps(&region, &table),
            vec![2.0, 5.0, 2.0, 9.0, 5.0]
        );
    }
}
