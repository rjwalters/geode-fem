//! Host-side p=2 volume assembly of the order-generic [`DrivenOperator`]
//! (issue #838, Epic #836 Phase 1a).
//!
//! The p=1 operator is assembled by the Burn-tensor pipeline in the parent
//! module and is untouched. This module builds the same ω-independent
//! ingredients — interior-filtered `K`, `M(ε)`, `C(σ)` values aligned with
//! a sparsity pattern, plus the full-length source moments — for a p=2
//! [`HcurlSpace`], in plain `f64` / faer form:
//!
//! - the element is evaluated on the tet's **ascending-sorted** vertices and
//!   every DOF scatters with **unit sign** (the ascending-global-vertex
//!   convention of [`crate::elements::nedelec_p2`]; never the p=1
//!   `sign_outer_tensor`, #616 red flag 1);
//! - coefficients are per-tet constant, so the degree-≥4 rule is exact for
//!   `M(ε)`, `M(σ)` and `K(ν)` and every tensor variant (#836 principle 6);
//! - the sparsity pattern is the interior DOF graph (row-sorted CSR, one
//!   slot per coupled interior pair), built through a DOF → tet incidence
//!   so peak memory is `O(nnz)`, not `O(n_tets · 400)` triplets.
//!
//! Surface terms (issue #857, Phase 1b) use the 8-DOF p=2 tangential-trace
//! kernel of [`crate::assembly::surface_p2`]: every lumped-port surface mass
//! and every impedance-surface mass couples only DOFs of the tet owning the
//! boundary face, so its entries are a subset of the volume pattern and are
//! cached aligned with it (surfaces) or as interior-remapped triplets
//! (ports), exactly as the p=1 operator stores them. The port flux
//! functional is full length (`n_dofs`), so the operator's port drive and
//! voltage readout are order-agnostic.

use faer::c64;

use super::{
    DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource, ElementOrder,
    MaterialsKind, OperatorPort, OperatorSurface, SurfaceImpedanceBc, validate_driven_surfaces,
    validate_lumped_ports,
};
use crate::assembly::hcurl_space::{HcurlSpace, TetOrientation};
use crate::assembly::surface_p2::{assemble_p2_port_flux, assemble_p2_surface_mass_triplets};
use crate::driven::ports::LumpedPort;
use crate::assembly::nedelec_p2::{p2_local_curl_tensor, p2_local_mass_tensor, tabulate_p2_tet};
use crate::elements::nedelec::{TET_QUAD4_A, TET_QUAD4_B};
use crate::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS as ND, tet_barycentric_gradients, tet_nedelec2_local, tet_nedelec2_local_rhs,
    tet_nedelec2_shapes, tet_quad_deg4,
};
use crate::mesh::TetMesh;

type LocalC = [[c64; ND]; ND];

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

/// Assemble the ω-independent p=2 operator. Inputs are validated with the
/// same errors as the p=1 [`DrivenOperator::assemble`].
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    source: DrivenSource<'_>,
) -> Result<DrivenOperator, DrivenError> {
    debug_assert_eq!(space.order(), ElementOrder::P2);
    let n_tets = mesh.n_tets();
    let n_dofs = space.n_dofs();

    // --- Input validation (mirrors the p=1 assemble_impl) -------------------
    if bcs.pec_interior_mask.len() != n_dofs {
        return Err(DrivenError::MaskDimMismatch {
            got: bcs.pec_interior_mask.len(),
            want: n_dofs,
        });
    }
    let source_len = match source {
        DrivenSource::Constant(s) => Some(s.j_tet.len()),
        DrivenSource::Quad(s) => Some(s.j_quad.len()),
        DrivenSource::Function(_) => None,
    };
    if let Some(len) = source_len
        && len != n_tets
    {
        return Err(DrivenError::SourceDimMismatch {
            got: len,
            want: n_tets,
        });
    }
    let (materials_kind, material_len) = match materials {
        DrivenMaterials::Scalar(eps) => (MaterialsKind::Scalar, eps.len()),
        DrivenMaterials::DiagTensor(eps) => (MaterialsKind::DiagTensor, eps.len()),
        DrivenMaterials::MatchedUpml {
            epsilon_tensor,
            nu_tensor,
        } => {
            if nu_tensor.len() != n_tets {
                return Err(DrivenError::MaterialDimMismatch {
                    got: nu_tensor.len(),
                    want: n_tets,
                });
            }
            (MaterialsKind::MatchedUpml, epsilon_tensor.len())
        }
    };
    if material_len != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: material_len,
            want: n_tets,
        });
    }
    if let Some(sigma) = sigma_tet
        && sigma.len() != n_tets
    {
        return Err(DrivenError::SigmaDimMismatch {
            got: sigma.len(),
            want: n_tets,
        });
    }
    // Ports and surfaces: the p=1 checks, then every triangle must be a
    // tet face (issue #725) — the trace kernel needs its edges and face in
    // the space's tables.
    validate_lumped_ports(mesh, ports)?;
    validate_driven_surfaces(
        mesh,
        "impedance surface",
        surfaces.iter().map(|bc| bc.triangles),
    )?;
    validate_driven_surfaces(mesh, "lumped port", ports.iter().map(|p| p.faces))?;

    // --- PEC reduction ------------------------------------------------------
    let mut remap = vec![-1_i64; n_dofs];
    let mut n_interior = 0usize;
    for (g, &keep) in bcs.pec_interior_mask.iter().enumerate() {
        if keep {
            remap[g] = n_interior as i64;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }

    // --- Interior sparsity pattern (row-sorted CSR) -------------------------
    let (row_ptr, col_idx) = interior_pattern(space, &remap, n_interior);
    let nnz = col_idx.len();
    let mut k_vals = vec![ZERO; nnz];
    let mut m_vals = vec![ZERO; nnz];
    let mut c_vals = sigma_tet.map(|_| vec![0.0_f64; nnz]);
    let mut rhs = vec![ZERO; n_dofs];

    let rule = tet_quad_deg4();
    for t in 0..n_tets {
        let coords = space.tet_local_coords(mesh, t);
        let dofs = space.tet_dofs(t);

        // Local K, M(ε) and the unweighted mass for C(σ).
        let need_m0 = sigma_tet.is_some() || matches!(materials, DrivenMaterials::Scalar(_));
        let (k_loc, m_loc, m0): (LocalC, LocalC, Option<[[f64; ND]; ND]>) = match materials {
            DrivenMaterials::Scalar(eps) => {
                let (k, m, _) = tet_nedelec2_local(&coords);
                let e = eps[t];
                (
                    std::array::from_fn(|i| std::array::from_fn(|j| c64::new(k[i][j], 0.0))),
                    std::array::from_fn(|i| std::array::from_fn(|j| e * m[i][j])),
                    Some(m),
                )
            }
            DrivenMaterials::DiagTensor(eps) => {
                let (k, m, _) = tet_nedelec2_local(&coords);
                let d = eps[t];
                let w = [[d[0], ZERO, ZERO], [ZERO, d[1], ZERO], [ZERO, ZERO, d[2]]];
                let tab = tabulate_p2_tet(&coords);
                (
                    std::array::from_fn(|i| std::array::from_fn(|j| c64::new(k[i][j], 0.0))),
                    p2_local_mass_tensor(&tab, &w),
                    need_m0.then_some(m),
                )
            }
            DrivenMaterials::MatchedUpml {
                epsilon_tensor,
                nu_tensor,
            } => {
                let tab = tabulate_p2_tet(&coords);
                let m0 = need_m0.then(|| tet_nedelec2_local(&coords).1);
                (
                    p2_local_curl_tensor(&tab, &nu_tensor[t]),
                    p2_local_mass_tensor(&tab, &epsilon_tensor[t]),
                    m0,
                )
            }
        };

        // Unit-sign scatter into the interior pattern.
        for i in 0..ND {
            let ri = remap[dofs[i] as usize];
            if ri < 0 {
                continue;
            }
            let ri = ri as usize;
            let row = &col_idx[row_ptr[ri]..row_ptr[ri + 1]];
            for j in 0..ND {
                let rj = remap[dofs[j] as usize];
                if rj < 0 {
                    continue;
                }
                let slot = row_ptr[ri]
                    + row
                        .binary_search(&(rj as u32))
                        .expect("coupled interior pair must be in the pattern");
                k_vals[slot] += k_loc[i][j];
                m_vals[slot] += m_loc[i][j];
                if let (Some(c), Some(sigma), Some(m0)) = (c_vals.as_mut(), sigma_tet, m0.as_ref())
                {
                    c[slot] += sigma[t] * m0[i][j];
                }
            }
        }

        // Full-length source moments ∫ N_i · J dV (scaled by iω per ω).
        let b_loc = local_rhs(space, mesh, t, &coords, source, &rule);
        for (i, &b) in b_loc.iter().enumerate() {
            rhs[dofs[i] as usize] += b;
        }
    }

    let mut rows = Vec::with_capacity(nnz);
    for r in 0..n_interior {
        rows.extend(std::iter::repeat_n(r, row_ptr[r + 1] - row_ptr[r]));
    }
    let cols: Vec<usize> = col_idx.iter().map(|&c| c as usize).collect();

    // --- Surface terms on the p=2 trace (issue #857) -------------------------
    // Impedance surfaces: S_Γ aligned with the interior pattern (its scalar
    // iω/Z_s(ω) is applied per ω by `assemble_a_at`, as at p=1).
    let operator_surfaces = surfaces
        .iter()
        .enumerate()
        .map(|(index, bc)| {
            let triplets = assemble_p2_surface_mass_triplets(space, mesh, bc.triangles)
                .map_err(|tri| not_on_mesh("impedance surface", index, tri, bc.triangles))?;
            let mut s_vals = vec![0.0_f64; nnz];
            for (r, c, v) in triplets {
                let (rr, cc) = (remap[r], remap[c]);
                if rr < 0 || cc < 0 {
                    continue;
                }
                s_vals[slot(&row_ptr, &col_idx, rr as usize, cc as usize)] += v;
            }
            Ok(OperatorSurface {
                s_vals,
                model: bc.model,
            })
        })
        .collect::<Result<Vec<_>, DrivenError>>()?;

    // Lumped ports: interior-remapped S_p triplets and the full-length flux
    // functional f_i = ∮ N_i · ê dS (drive and voltage readout).
    let operator_ports = ports
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let flux = assemble_p2_port_flux(space, mesh, port.faces, port.e_hat)
                .map_err(|tri| not_on_mesh("lumped port", index, tri, port.faces))?;
            let mass_triplets = assemble_p2_surface_mass_triplets(space, mesh, port.faces)
                .map_err(|tri| not_on_mesh("lumped port", index, tri, port.faces))?
                .into_iter()
                .filter_map(|(r, c, v)| {
                    let (rr, cc) = (remap[r], remap[c]);
                    (rr >= 0 && cc >= 0).then_some((rr as usize, cc as usize, v))
                })
                .collect();
            Ok(OperatorPort {
                mass_triplets,
                flux,
                z_s: port.surface_impedance(),
                v_inc: port.v_inc,
                length: port.length,
                width: port.width,
                resistance: port.resistance,
            })
        })
        .collect::<Result<Vec<_>, DrivenError>>()?;

    Ok(DrivenOperator {
        order: ElementOrder::P2,
        n_dofs,
        n_interior,
        remap,
        pec_interior_mask: bcs.pec_interior_mask.to_vec(),
        rows,
        cols,
        k_vals,
        m_vals,
        c_vals,
        surfaces: operator_surfaces,
        ports: operator_ports,
        rhs_re: rhs.iter().map(|b| b.re).collect(),
        rhs_im: rhs.iter().map(|b| b.im).collect(),
        materials_kind,
        // The matrix-free and AMS paths are p=1-only; `prepare_at` rejects
        // them for a p=2 operator before these are consulted.
        matrix_free: None,
        ams_geometry: None,
    })
}

/// The local p=2 source moments `∫_T N_i · J dV` of tet `t` (sorted-vertex
/// layout) for each [`DrivenSource`] variant.
fn local_rhs(
    space: &HcurlSpace,
    mesh: &TetMesh,
    t: usize,
    coords: &[[f64; 3]; 4],
    source: DrivenSource<'_>,
    rule: &[([f64; 4], f64)],
) -> [c64; ND] {
    match source {
        DrivenSource::Constant(src) => {
            let j = src.j_tet[t];
            let re = tet_nedelec2_local_rhs(coords, [j[0].re, j[1].re, j[2].re]);
            let im = tet_nedelec2_local_rhs(coords, [j[0].im, j[1].im, j[2].im]);
            std::array::from_fn(|i| c64::new(re[i], im[i]))
        }
        DrivenSource::Quad(src) => {
            // The four stored degree-2 samples: point q has barycentric
            // TET_QUAD4_A on natural vertex q and TET_QUAD4_B elsewhere,
            // weight |V|/4 each.
            let perm = match space.tet_orientation(t) {
                TetOrientation::AscendingPerm(p) => p,
                TetOrientation::EdgeSigns(_) => unreachable!("p=2 space"),
            };
            let (grad, vol) = tet_barycentric_gradients(coords);
            let w = vol.abs() / 4.0;
            let mut b = [ZERO; ND];
            for (q, jq) in src.j_quad[t].iter().enumerate() {
                let lam_nat: [f64; 4] =
                    std::array::from_fn(|v| if v == q { TET_QUAD4_A } else { TET_QUAD4_B });
                let lam: [f64; 4] = std::array::from_fn(|i| lam_nat[perm[i]]);
                let (n, _c) = tet_nedelec2_shapes(&lam, &grad);
                for (i, bi) in b.iter_mut().enumerate() {
                    *bi += (jq[0] * n[i][0] + jq[1] * n[i][1] + jq[2] * n[i][2]) * w;
                }
            }
            b
        }
        DrivenSource::Function(f) => {
            let (grad, vol) = tet_barycentric_gradients(coords);
            let vol_abs = vol.abs();
            let mut b = [ZERO; ND];
            for (lam, frac) in rule {
                let x: [f64; 3] =
                    std::array::from_fn(|d| (0..4).map(|p| lam[p] * coords[p][d]).sum::<f64>());
                let j = f(t, x);
                let (n, _c) = tet_nedelec2_shapes(lam, &grad);
                let w = vol_abs * frac;
                for (i, bi) in b.iter_mut().enumerate() {
                    *bi += (j[0] * n[i][0] + j[1] * n[i][1] + j[2] * n[i][2]) * w;
                }
            }
            let _ = mesh;
            b
        }
    }
}

/// The pattern slot of interior pair `(r, c)`. Every surface pair lies in
/// the pattern: a boundary face's 8 trace DOFs are DOFs of the tet owning
/// the face.
fn slot(row_ptr: &[usize], col_idx: &[u32], r: usize, c: usize) -> usize {
    let row = &col_idx[row_ptr[r]..row_ptr[r + 1]];
    row_ptr[r]
        + row
            .binary_search(&(c as u32))
            .expect("surface pair must lie within the volume pattern")
}

/// [`DrivenError::SurfaceNotOnMesh`] for a triangle the p=2 trace kernel
/// could not place (defensive: [`validate_driven_surfaces`] has already
/// rejected non-faces).
fn not_on_mesh(kind: &str, index: usize, tri: [u32; 3], all: &[[u32; 3]]) -> DrivenError {
    DrivenError::SurfaceNotOnMesh {
        surface: format!("{kind} {index}"),
        triangle: tri,
        dangling: 1,
        total: all.len(),
    }
}

/// Row-sorted CSR pattern of the interior DOF graph: interior DOFs `r`, `c`
/// are coupled iff some tet carries both. Built through a DOF → tet
/// incidence, `O(nnz)` memory.
fn interior_pattern(
    space: &HcurlSpace,
    remap: &[i64],
    n_interior: usize,
) -> (Vec<usize>, Vec<u32>) {
    let n_tets = space.n_tets();
    // Interior DOF → incident tets (CSR).
    let mut inc_ptr = vec![0usize; n_interior + 1];
    for t in 0..n_tets {
        for &d in space.tet_dofs(t) {
            let r = remap[d as usize];
            if r >= 0 {
                inc_ptr[r as usize + 1] += 1;
            }
        }
    }
    for r in 0..n_interior {
        inc_ptr[r + 1] += inc_ptr[r];
    }
    let mut fill = inc_ptr.clone();
    let mut inc = vec![0u32; inc_ptr[n_interior]];
    for t in 0..n_tets {
        for &d in space.tet_dofs(t) {
            let r = remap[d as usize];
            if r >= 0 {
                inc[fill[r as usize]] = t as u32;
                fill[r as usize] += 1;
            }
        }
    }

    let mut row_ptr = Vec::with_capacity(n_interior + 1);
    row_ptr.push(0usize);
    let mut cols: Vec<u32> = Vec::new();
    let mut scratch: Vec<u32> = Vec::new();
    for r in 0..n_interior {
        scratch.clear();
        for &t in &inc[inc_ptr[r]..inc_ptr[r + 1]] {
            for &d in space.tet_dofs(t as usize) {
                let c = remap[d as usize];
                if c >= 0 {
                    scratch.push(c as u32);
                }
            }
        }
        scratch.sort_unstable();
        scratch.dedup();
        cols.extend_from_slice(&scratch);
        row_ptr.push(cols.len());
    }
    (row_ptr, cols)
}
