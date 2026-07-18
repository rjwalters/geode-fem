//! Order-selectable **lossless PEC-cube cavity** generalized eigenproblem
//! `K x = λ M x` (Epic #475 parity gap #3, Epic #569; issue #620, follow-on
//! to #616/#621).
//!
//! This is the eigenmode-path consumer of the opt-in second-order (`p=2`)
//! Nédélec global assembly that #621 landed in
//! [`crate::assembly::nedelec_p2`]. It threads an [`ElementOrder`] switch
//! (`p=1` | `p=2`) through a single lossless-cavity eigensolve entry point
//! so the `p=2` curl-curl / mass pencil can be measured against the analytic
//! PEC-cube spectrum on the **same** solver (the sparse shift-invert Lanczos
//! [`crate::eigen::lanczos::SparseShiftInvertLanczos`]) as `p=1`.
//!
//! # Scope (deliberately the lossless cavity, not the reactive-shunt transmon)
//!
//! The reactive-shunt transmon eigensolve
//! ([`crate::eigen::transmon::TransmonPencil`], `solve_transmon_eigenmodes*`)
//! is built entirely on **`p=1` edge DOFs**: the junction
//! [`crate::eigen::transmon::LumpedReactiveShunt`] port surface mass
//! ([`crate::driven::ports::assemble_port_surface_mass`]), the tree-cotree
//! gauge ([`crate::eigen::gauge`]), and the mode participation are all
//! `p=1`-edge-specific and have **no `p=2` analogue on `main`**. A full
//! `p=2` transmon-with-junction eigensolve would require three new
//! subsystems (a `p=2` port surface mass, a `p=2` gauge, and `p=2`
//! participation) — that is a separate, larger follow-on and is **out of
//! scope here**. This module is the lossless PEC-cavity path the issue's
//! acceptance criteria (frequency-convergence gate; `p=1` byte-identical)
//! actually require, and it leaves `TransmonPencil` byte-identical.
//!
//! # Gradient nullspace
//!
//! The curl-curl stiffness `K` has a non-trivial nullspace — the gradients
//! of the scalar (Lagrange) potential space with the same Dirichlet BC. At
//! `p=1` its interior dimension equals the interior-node count; at `p=2` it
//! is **larger** (interior nodes + interior edges of the `P2`-Lagrange
//! space), so the shift-invert eigensolve surfaces more near-zero spurious
//! modes. The shift `sigma` must be placed strictly **above 0 and below the
//! first physical eigenvalue** so `A = K − sigma·M` is non-singular (the LU
//! never sees the singular `K` at `sigma = 0`) and the physical band is
//! separated from the near-zero cluster. [`CavityModes::physical`] filters
//! the near-zero cluster by magnitude; see the `*_eigen_p2_convergence`
//! gate for the analytic `2π²` oracle it is validated against.

use burn::tensor::backend::Backend;
use faer::MatRef;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::nedelec::{assemble_global_nedelec, cube_pec_interior_edges};
use crate::assembly::nedelec_p2::{P2DofMap, cube_pec_interior_p2_dofs, p2_interior_km};
use crate::assembly::p1::upload_mesh;
use crate::eigen::dense::{EigenError, apply_dirichlet_bc, burn_matrix_to_faer};
use crate::eigen::lanczos::SparseShiftInvertLanczos;
use crate::mesh::TetMesh;

/// Finite-element order for the lossless-cavity eigensolve.
///
/// `P1` is the first-order Whitney edge element (the historical default —
/// selecting it reproduces the existing `p=1` eigen path byte-for-byte).
/// `P2` routes through the #621 second-order Nédélec global assembly
/// ([`crate::assembly::nedelec_p2`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementOrder {
    /// First-order Whitney edge element (6 DOFs / tet). Default.
    #[default]
    P1,
    /// Second-order Nédélec element (20 DOFs / tet), via #621's global
    /// `edges×2 + faces×2` assembly.
    P2,
}

/// Converged lossless-cavity eigenvalues plus the reduced (interior) DOF
/// count of the pencil they came from.
///
/// `lambdas` is ascending and **includes** the near-zero gradient-nullspace
/// cluster; call [`CavityModes::physical`] to filter it and recover the
/// physical spectrum.
#[derive(Debug, Clone)]
pub struct CavityModes {
    /// All converged eigenvalues `λ = k²` (in `(1/mesh-unit)²`), ascending.
    pub lambdas: Vec<f64>,
    /// Interior (kept, non-PEC) DOF count of the reduced pencil.
    pub n_interior: usize,
}

impl CavityModes {
    /// Physical eigenvalues: those strictly above `null_tol`, ascending.
    ///
    /// The curl-free gradient near-kernel clusters near `0`; any
    /// `null_tol` placed between that cluster and the first physical
    /// eigenvalue (here the analytic `2π²`) recovers the physical spectrum.
    pub fn physical(&self, null_tol: f64) -> Vec<f64> {
        self.lambdas
            .iter()
            .copied()
            .filter(|&l| l > null_tol)
            .collect()
    }

    /// The lowest physical eigenvalue (smallest above `null_tol`), or `None`
    /// if every converged eigenvalue is in the near-zero cluster.
    pub fn first_physical(&self, null_tol: f64) -> Option<f64> {
        self.physical(null_tol).into_iter().next()
    }
}

/// An interior-reduced cavity pencil: `(K_int, M_int, n_interior)`.
type CavityPencil = (SparseColMat<usize, f64>, SparseColMat<usize, f64>, usize);

/// Convert a dense faer matrix into a sparse `SparseColMat`, keeping only the
/// structurally non-zero entries (off-stencil entries in the assembled
/// curl-curl / mass matrices are exactly `0.0` and are dropped).
fn dense_to_sparse(a: MatRef<'_, f64>) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = a.nrows();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for j in 0..a.ncols() {
        for i in 0..n {
            let v = a[(i, j)];
            if v != 0.0 {
                trips.push(Triplet::new(i, j, v));
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(n, a.ncols(), &trips)
        .map_err(|e| EigenError::FaerGevd(format!("dense→sparse cavity pencil: {e:?}")))
}

/// Assemble the `p=1` PEC-cube interior curl-curl / mass sparse pencil.
///
/// Reuses the existing first-order path
/// ([`assemble_global_nedelec`] + [`cube_pec_interior_edges`] +
/// [`apply_dirichlet_bc`]) unchanged, then projects the interior-reduced
/// dense matrices to sparse for the shared Lanczos solver. Returns
/// `(K_int, M_int, n_interior)`.
fn cavity_interior_km_p1<B: Backend>(
    mesh: &TetMesh,
    side: f64,
    device: &B::Device,
) -> Result<CavityPencil, EigenError> {
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);

    let tet_edges_v = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let sys = assemble_global_nedelec(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);

    let (_edges, edge_mask) = cube_pec_interior_edges(mesh, side);
    let (k_int, m_int) = apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &edge_mask)?;
    let n_interior = k_int.nrows();

    let k_sp = dense_to_sparse(k_int.as_ref())?;
    let m_sp = dense_to_sparse(m_int.as_ref())?;
    Ok((k_sp, m_sp, n_interior))
}

/// Assemble the `p=2` PEC-cube interior curl-curl / mass sparse pencil.
///
/// Reuses #621's global assembly ([`P2DofMap`], [`p2_interior_km`],
/// [`cube_pec_interior_p2_dofs`]) directly — no assembly is re-implemented
/// here. The lossless cavity is `ε = 1` per tet. Returns
/// `(K_int, M_int, n_interior)`.
fn cavity_interior_km_p2(mesh: &TetMesh, side: f64) -> Result<CavityPencil, EigenError> {
    let dofs = P2DofMap::build(mesh);
    let eps_tet = vec![1.0_f64; mesh.n_tets()];
    let interior_mask = cube_pec_interior_p2_dofs(mesh, &dofs, side);
    let (_remap, n_interior, kept) = p2_interior_km(mesh, &dofs, &eps_tet, &interior_mask);

    let mut k_tr: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(kept.len());
    let mut m_tr: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(kept.len());
    for (ri, rj, k_local, m_local) in kept {
        k_tr.push(Triplet::new(ri, rj, k_local));
        m_tr.push(Triplet::new(ri, rj, m_local));
    }
    let k_sp = SparseColMat::<usize, f64>::try_new_from_triplets(n_interior, n_interior, &k_tr)
        .map_err(|e| EigenError::FaerGevd(format!("p=2 cavity K: {e:?}")))?;
    let m_sp = SparseColMat::<usize, f64>::try_new_from_triplets(n_interior, n_interior, &m_tr)
        .map_err(|e| EigenError::FaerGevd(format!("p=2 cavity M: {e:?}")))?;
    Ok((k_sp, m_sp, n_interior))
}

/// Solve the lossless PEC-cube cavity generalized eigenproblem at the
/// selected [`ElementOrder`].
///
/// Builds the interior-reduced curl-curl / mass pencil (`p=1` via the
/// existing Whitney path, `p=2` via #621's second-order assembly), then runs
/// the shared sparse shift-invert Lanczos
/// ([`SparseShiftInvertLanczos`]) near `sigma`. Returns the converged
/// eigenvalues (ascending, near-zero gradient cluster included — filter with
/// [`CavityModes::physical`]).
///
/// `sigma` **must** sit strictly above `0` and below the first physical
/// eigenvalue so `A = K − sigma·M` is non-singular and the physical band
/// converges ahead of the near-zero gradient nullspace (see the module
/// docs). `n_modes` is clamped to the interior DOF count.
///
/// The backend `B` is only exercised by the `p=1` assembly; the `p=2` path
/// is pure host `f64`.
///
/// # Errors
///
/// Propagates [`EigenError`] from the interior reduction, the sparse
/// projection, or the Lanczos solve.
pub fn solve_pec_cube_cavity_modes<B: Backend>(
    mesh: &TetMesh,
    side: f64,
    device: &B::Device,
    order: ElementOrder,
    sigma: f64,
    n_modes: usize,
) -> Result<CavityModes, EigenError> {
    let (k, m, n_interior) = match order {
        ElementOrder::P1 => cavity_interior_km_p1::<B>(mesh, side, device)?,
        ElementOrder::P2 => cavity_interior_km_p2(mesh, side)?,
    };

    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters: 128,
        tol: 1e-9,
        ..Default::default()
    };
    let want = n_modes.min(n_interior);
    let pairs = solver.smallest_eigenpairs(k.as_ref(), m.as_ref(), want)?;

    let mut lambdas: Vec<f64> = pairs.iter().map(|p| p.lambda).collect();
    lambdas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(CavityModes {
        lambdas,
        n_interior,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type B = TestBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    /// The `p=2` interior pencil has strictly more DOFs than the `p=1`
    /// pencil on the same cube (20 vs 6 DOFs/tet), and both produce a
    /// non-empty physical spectrum near the analytic first mode `2π²`.
    #[test]
    fn p1_and_p2_cavity_pencils_build_and_separate_nullspace() {
        let side = 1.0;
        let mesh = cube_tet_mesh(3, side);
        let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);
        // sigma between the near-zero nullspace and the first physical mode,
        // closer to 2π² so the physical band converges first.
        let sigma = 0.7 * two_pi2;
        let null_tol = 0.5 * two_pi2;

        let (_kp1, _mp1, n_int_p1) = cavity_interior_km_p1::<B>(&mesh, side, &device()).unwrap();
        let (_kp2, _mp2, n_int_p2) = cavity_interior_km_p2(&mesh, side).unwrap();
        assert!(
            n_int_p2 > n_int_p1,
            "p=2 interior DOFs {n_int_p2} should exceed p=1 {n_int_p1}"
        );

        let modes_p1 =
            solve_pec_cube_cavity_modes::<B>(&mesh, side, &device(), ElementOrder::P1, sigma, 6)
                .unwrap();
        let modes_p2 =
            solve_pec_cube_cavity_modes::<B>(&mesh, side, &device(), ElementOrder::P2, sigma, 6)
                .unwrap();

        let f1 = modes_p1
            .first_physical(null_tol)
            .expect("p=1 physical mode");
        let f2 = modes_p2
            .first_physical(null_tol)
            .expect("p=2 physical mode");
        // Both land in the neighbourhood of the analytic first mode.
        assert!(
            (f1 - two_pi2).abs() / two_pi2 < 0.5,
            "p=1 first physical {f1} not near 2π² = {two_pi2}"
        );
        assert!(
            (f2 - two_pi2).abs() / two_pi2 < 0.5,
            "p=2 first physical {f2} not near 2π² = {two_pi2}"
        );
    }
}
