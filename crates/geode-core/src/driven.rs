//! Deterministic **driven** frequency-domain solve `A(ω) x = b` with a
//! volumetric current source (Epic #193, issue #194).
//!
//! Where the eigenpencil path solves `(K − k² M) x = 0` for resonances,
//! this module solves the *forced* problem at a prescribed frequency ω:
//!
//! ```text
//! A(ω) x = b,        A(ω) = K − (ω/c)² M(ε),
//! b_i = i ω μ₀ ∫_Ω N_i · J dV,
//! ```
//!
//! in the codebase's natural units (`c = μ₀ = ε₀ = 1`, so `ω ≡ k₀` and
//! eigenvalues of the pencil are `k²`). `K` is the Nédélec curl-curl
//! stiffness, `M(ε)` the (possibly complex / anisotropic-diagonal) mass,
//! and `J` a per-tet piecewise-constant current density.
//!
//! # Boundary conditions
//!
//! BCs compose with the driven system exactly as they do with the
//! eigenpencil:
//!
//! - **PEC** — row/column elimination via a per-edge interior mask
//!   (same mask helpers as the eigen path:
//!   [`crate::nedelec_assembly::pec_interior_edge_mask`],
//!   [`crate::nedelec_assembly::cube_pec_interior_edges`],
//!   [`crate::nedelec_assembly::sphere_pec_interior_edges`]). Eliminated
//!   edge DOFs are returned as exact zeros in the full-length solution.
//! - **UPML / scalar PML** — enters through the *material*: a complex
//!   scalar ε ([`crate::nedelec_assembly::build_complex_epsilon_r_pml`])
//!   or a diagonal anisotropic tensor ε
//!   ([`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`])
//!   makes `M` complex, which makes `A(ω)` invertible for real ω and
//!   absorbs outgoing radiation.
//!
//! # Solver
//!
//! Reuses the existing sparse complex factorization machinery from the
//! shift-and-invert Lanczos path ([`crate::complex_lanczos`]): the
//! interior-reduced `A(ω)` is built as a `faer` sparse CSC matrix from
//! the assembly [`crate::assembly::SparsityPattern`], factored once with
//! `sp_lu`, and solved directly. A direct sparse solve is sufficient at
//! the mesh sizes this crate targets; iterative solvers are out of scope
//! (issue #194).
//!
//! # Sign / time convention
//!
//! `exp(+jωt)` time convention, consistent with the rest of the codebase
//! (see `silvermuller.rs` and the PML builders, which produce
//! `Im(ε) < 0` for absorption). The strong form corresponding to the
//! discrete system above is
//!
//! ```text
//! ∇×∇×E − ω² ε E = i ω J,
//! ```
//!
//! i.e. the source phase convention follows the issue statement
//! (`b = iωμ₀ ∫ N · J`). A global phase flip of `J` only flips the phase
//! of `x`; absorption direction is set by `Im(ε)` in `A`, not by `b`.
//!
//! # Autodiff
//!
//! The assembly of `K`, `M`, and `b` runs through the batched-local-
//! kernel + `scatter(Add)` Burn path and preserves autodiff up to the
//! host transfer. The sparse factorization itself is faer (CPU) and
//! breaks the tape — same trade-off as the eigensolver layer
//! ([`crate::eigen`]).

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use crate::complex_lanczos::{solve_with_lu, spmv};
use crate::eigen::burn_matrix_to_faer;
use crate::nedelec_assembly::{
    assemble_global_nedelec_with_anisotropic_epsilon, assemble_global_nedelec_with_complex_epsilon,
    assemble_nedelec_current_rhs, burn_complex_mass_to_faer, tet_centroids,
};
use crate::TetMesh;

/// Errors produced by the driven-solve layer.
#[derive(Debug, thiserror::Error)]
pub enum DrivenError {
    #[error("PEC interior mask length {got} disagrees with edge count {want}")]
    MaskDimMismatch { got: usize, want: usize },
    #[error("source has {got} per-tet entries but mesh has {want} tets")]
    SourceDimMismatch { got: usize, want: usize },
    #[error("material has {got} per-tet entries but mesh has {want} tets")]
    MaterialDimMismatch { got: usize, want: usize },
    #[error("driven system has no interior DOFs after PEC elimination")]
    EmptyInterior,
    #[error("sparse system assembly failed: {0}")]
    SparseAssembly(String),
    #[error("sparse LU factorization of A(ω) failed: {0}")]
    Factorization(String),
    #[error("sparse solve failed: {0}")]
    Solve(String),
}

/// Per-tet material description for the driven solve.
///
/// Mirrors the material inputs the eigenpencil path accepts: a complex
/// scalar relative permittivity per tet, or the diagonal-anisotropic
/// UPML tensor per tet. The stiffness `K` is permittivity-independent
/// in both cases.
#[derive(Debug, Clone, Copy)]
pub enum DrivenMaterials<'a> {
    /// Scalar complex relative permittivity per tet (`ε_r ∈ ℂ`). Use
    /// real values for plain dielectrics / PEC cavities and
    /// [`crate::nedelec_assembly::build_complex_epsilon_r_pml`] for the
    /// scalar-PML profile.
    Scalar(&'a [c64]),
    /// Diagonal anisotropic complex permittivity per tet in the global
    /// Cartesian basis (`[ε_x, ε_y, ε_z]`), as produced by
    /// [`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`]
    /// for the UPML shell.
    DiagTensor(&'a [[c64; 3]]),
}

/// Boundary conditions for the driven solve.
///
/// Currently the PEC interior-edge mask; the UPML/scalar-PML absorbing
/// boundary is a *material* (see [`DrivenMaterials`]) and composes with
/// the PEC outer wall exactly as in the eigenpencil tests.
#[derive(Debug, Clone)]
pub struct DrivenBcs<'a> {
    /// Per-edge mask over `mesh.edges()` order: `true` = kept interior
    /// DOF, `false` = PEC-eliminated edge (forced to zero).
    pub pec_interior_mask: &'a [bool],
}

/// Volumetric current source, piecewise constant per tet.
#[derive(Debug, Clone)]
pub struct CurrentSource {
    /// `[n_tets][3]` complex current density `J` per tet, in `mesh.tets`
    /// order.
    pub j_tet: Vec<[c64; 3]>,
}

impl CurrentSource {
    /// Sample a continuous current density `J(x)` at every tet centroid.
    pub fn from_centroids(mesh: &TetMesh, f: impl Fn([f64; 3]) -> [c64; 3]) -> Self {
        let j_tet = tet_centroids(mesh).into_iter().map(f).collect();
        Self { j_tet }
    }
}

/// Solution of the driven system.
#[derive(Debug, Clone)]
pub struct DrivenSolution {
    /// Full-length `[n_edges]` complex edge-DOF vector in `mesh.edges()`
    /// order. PEC-eliminated edges carry exact zeros.
    pub e_edges: Vec<c64>,
    /// Number of interior (kept) DOFs after PEC elimination.
    pub n_interior: usize,
    /// Relative residual `‖A x − b‖₂ / ‖b‖₂` of the interior system,
    /// computed post-solve as a numerical health check. For a healthy
    /// direct sparse solve this is at the round-off floor.
    pub residual_rel: f64,
}

/// Deterministic driven frequency-domain solve `A(ω) x = b` with a
/// volumetric current source (issue #194).
///
/// Assembles the Nédélec curl-curl pencil `(K, M(ε))` on `mesh`, the
/// current-source RHS `b_i = iω ∫ N_i · J dV` (natural units, `μ₀ = 1`),
/// applies the PEC reduction from `bcs`, factors the interior
/// `A(ω) = K − ω² M` once with faer's complex sparse LU, and returns the
/// full-length edge field with zeros on eliminated edges.
///
/// `omega` is `ω/c = k₀` in the mesh's length units (the same
/// normalization in which the eigenpencil's eigenvalues are `k²`).
///
/// # Errors
///
/// Returns [`DrivenError`] on input-shape mismatches or if the sparse
/// factorization/solve fails (e.g. `ω²` collides with a real eigenvalue
/// of a lossless pencil, making `A(ω)` singular).
pub fn driven_solve<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    let n_tets = mesh.n_tets();
    let edges = mesh.edges();
    let n_edges = edges.len();

    // --- Input validation -------------------------------------------------
    if bcs.pec_interior_mask.len() != n_edges {
        return Err(DrivenError::MaskDimMismatch {
            got: bcs.pec_interior_mask.len(),
            want: n_edges,
        });
    }
    if source.j_tet.len() != n_tets {
        return Err(DrivenError::SourceDimMismatch {
            got: source.j_tet.len(),
            want: n_tets,
        });
    }
    let material_len = match materials {
        DrivenMaterials::Scalar(eps) => eps.len(),
        DrivenMaterials::DiagTensor(eps) => eps.len(),
    };
    if material_len != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: material_len,
            want: n_tets,
        });
    }

    // --- Edge tables ------------------------------------------------------
    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    // --- Assemble K, M(ε) on the Burn backend ------------------------------
    let (nodes_t, tets_t) = crate::assembly::upload_mesh::<B>(mesh, device);
    let sys = match materials {
        DrivenMaterials::Scalar(eps) => assemble_global_nedelec_with_complex_epsilon(
            nodes_t.clone(),
            tets_t.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            eps,
        ),
        DrivenMaterials::DiagTensor(eps) => assemble_global_nedelec_with_anisotropic_epsilon(
            nodes_t.clone(),
            tets_t.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            eps,
        ),
    };

    // --- Assemble the current-source RHS -----------------------------------
    // The Burn RHS kernel is real-valued; a complex J runs as two passes
    // (Re(J), Im(J)) — same split as the complex-ε mass assembly.
    let j_re: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].re, j[1].re, j[2].re])
        .collect();
    let j_im: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].im, j[1].im, j[2].im])
        .collect();
    let rhs_re = assemble_nedelec_current_rhs(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &j_re,
    );
    let rhs_im = assemble_nedelec_current_rhs(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_im);
    let rhs_re: Vec<f64> = rhs_re.into_data().iter::<f64>().collect();
    let rhs_im: Vec<f64> = rhs_im.into_data().iter::<f64>().collect();

    // b = iωμ₀ ∫ N · J dV with μ₀ = 1:  iω (re + i·im) = ω(−im + i·re).
    let b_full: Vec<c64> = rhs_re
        .iter()
        .zip(rhs_im.iter())
        .map(|(&re, &im)| c64::new(-omega * im, omega * re))
        .collect();

    // --- Host transfer + PEC reduction -------------------------------------
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // Remap full edge indices → contiguous interior indices.
    let mut remap = vec![-1_i64; n_edges];
    let mut n_interior = 0_usize;
    for (i, &keep) in bcs.pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n_interior as i64;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }

    // --- Sparse A(ω) = K − ω² M over the recorded sparsity pattern ---------
    let omega2 = omega * omega;
    let mut triplets: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(sys.sparsity.rows.len());
    for (&r_u32, &c_u32) in sys.sparsity.rows.iter().zip(sys.sparsity.cols.iter()) {
        let (r, c) = (r_u32 as usize, c_u32 as usize);
        let (rr, cc) = (remap[r], remap[c]);
        if rr < 0 || cc < 0 {
            continue;
        }
        let a_val = c64::new(k_full[(r, c)], 0.0) - m_full[(r, c)] * omega2;
        triplets.push(Triplet::new(rr as usize, cc as usize, a_val));
    }
    let a_int =
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, &triplets)
            .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))?;

    let b_int: Vec<c64> = bcs
        .pec_interior_mask
        .iter()
        .zip(b_full.iter())
        .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
        .collect();

    // --- Factor once + direct solve (same machinery as complex Lanczos) ----
    let lu = a_int
        .as_ref()
        .sp_lu()
        .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
    let mut x_int = vec![c64::new(0.0, 0.0); n_interior];
    solve_with_lu(&lu, &b_int, &mut x_int).map_err(|e| DrivenError::Solve(format!("{e}")))?;

    // --- Post-solve residual check ------------------------------------------
    let mut ax = vec![c64::new(0.0, 0.0); n_interior];
    spmv(a_int.as_ref(), &x_int, &mut ax);
    let mut res2 = 0.0_f64;
    let mut b2 = 0.0_f64;
    for i in 0..n_interior {
        let r = ax[i] - b_int[i];
        res2 += r.re * r.re + r.im * r.im;
        b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
    }
    let residual_rel = if b2 > 0.0 {
        (res2 / b2).sqrt()
    } else {
        res2.sqrt()
    };

    // --- Scatter back to the full edge vector --------------------------------
    let mut e_edges = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            e_edges[full_idx] = x_int[ri as usize];
        }
    }

    Ok(DrivenSolution {
        e_edges,
        n_interior,
        residual_rel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nedelec_assembly::cube_pec_interior_edges;
    use crate::{cube_tet_mesh, DefaultBackend};
    use burn::tensor::backend::BackendTypes;

    type B = DefaultBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    fn vacuum(mesh: &TetMesh) -> Vec<c64> {
        vec![c64::new(1.0, 0.0); mesh.n_tets()]
    }

    /// A zero current source must produce an exactly-zero field.
    #[test]
    fn zero_source_gives_zero_field() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let sol = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &source,
            &device(),
        )
        .expect("driven solve");
        assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
    }

    /// Linearity: doubling J doubles the field (up to round-off).
    #[test]
    fn solution_is_linear_in_source() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let s1 = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let s2 = CurrentSource {
            j_tet: s1.j_tet.iter().map(|j| j.map(|x| x * 2.0)).collect(),
        };
        let omega = 1.0;
        let sol1 = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &s1,
            &device(),
        )
        .expect("solve s1");
        let sol2 = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &s2,
            &device(),
        )
        .expect("solve s2");

        let norm1: f64 = sol1
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        assert!(norm1 > 0.0, "nonzero source must excite a nonzero field");
        for (a, b) in sol1.e_edges.iter().zip(sol2.e_edges.iter()) {
            let d = *b - *a * 2.0;
            assert!(
                d.re.hypot(d.im) <= 1e-9 * norm1,
                "linearity violated: 2·{a} vs {b}"
            );
        }
    }

    /// The post-solve residual must sit at the direct-solve round-off
    /// floor, and PEC-eliminated edges must be exactly zero.
    #[test]
    fn residual_is_small_and_pec_edges_are_zero() {
        let mesh = cube_tet_mesh(3, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource::from_centroids(&mesh, |_| {
            [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
        });
        let sol = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.5,
            &source,
            &device(),
        )
        .expect("driven solve");
        assert!(
            sol.residual_rel < 1e-10,
            "direct-solve residual too large: {}",
            sol.residual_rel
        );
        for (i, &keep) in interior.iter().enumerate() {
            if !keep {
                assert_eq!(sol.e_edges[i], c64::new(0.0, 0.0));
            }
        }
        assert!(sol
            .e_edges
            .iter()
            .all(|e| e.re.is_finite() && e.im.is_finite()));
    }

    /// An isotropic DiagTensor material must agree with the Scalar
    /// material path (the two assembly kernels are independent
    /// implementations of the same integral).
    #[test]
    fn diag_tensor_matches_scalar_for_isotropic_epsilon() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps_scalar: Vec<c64> = vec![c64::new(1.8, -0.05); mesh.n_tets()];
        let eps_diag: Vec<[c64; 3]> = eps_scalar.iter().map(|&e| [e, e, e]).collect();
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(c[1], 0.0),
                c64::new(0.0, -0.5),
                c64::new(1.0, c[0]),
            ]
        });
        let omega = 2.0;
        let sol_s = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps_scalar),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("scalar solve");
        let sol_d = driven_solve::<B>(
            &mesh,
            DrivenMaterials::DiagTensor(&eps_diag),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("diag solve");

        let norm: f64 = sol_s
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        assert!(norm > 0.0);
        let mut max_rel = 0.0_f64;
        for (a, b) in sol_s.e_edges.iter().zip(sol_d.e_edges.iter()) {
            let d = *a - *b;
            max_rel = max_rel.max(d.re.hypot(d.im) / norm);
        }
        assert!(
            max_rel < 1e-4,
            "scalar vs diag-tensor isotropic mismatch: max relative diff {max_rel}"
        );
    }

    /// Shape-mismatch inputs must error, not panic.
    #[test]
    fn input_validation_errors() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let good_source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };

        let bad_mask = vec![true; 3];
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &bad_mask,
            },
            1.0,
            &good_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::MaskDimMismatch { .. }));

        let bad_source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; 2],
        };
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &bad_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::SourceDimMismatch { .. }));

        let bad_eps = vec![c64::new(1.0, 0.0); 1];
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&bad_eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &good_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::MaterialDimMismatch { .. }));
    }
}
