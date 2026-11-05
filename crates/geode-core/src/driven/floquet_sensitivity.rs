//! **Floquet-port S-matrix sensitivities** (Epic #1252 Phase 1, issue
//! #1256): `∂S_pq/∂θ` of the power-normalized S matrix of a
//! [`FloquetCell`] with respect to per-region permittivity `ε′`, `ε″` and
//! node-motion (shape) columns, FD-validated. The JVP table
//! ([`floquet_s_sensitivity`]) and the scalar VJP ([`floquet_s_vjp`])
//! mirror the entry points of [`crate::driven::s_sensitivity`] (#842) and
//! share its design types ([`MaterialDesign`], [`ShapeDesign`],
//! [`SParam`]).
//!
//! # The identity
//!
//! At one `(ω, k_t)` the cell solves the reduced, Hermitian-structured
//! system (see [`crate::driven::floquet`])
//!
//! ```text
//! A_r x_q = b_q,   A_r = Pᴴ A P + Σ_c c_c h̄_c h_cᵀ,   b_q = 2 c_q h̄_q,
//! S_pq = √(y_p/y_q) (h_pᵀ x_q − δ_pq),
//! ```
//!
//! with `P = P(k_t)` the Bloch-phased periodic prolongation. While the
//! port faces and the periodic faces are **pinned**, the port term, the
//! read-out vectors `h_p`, the drive `b_q` and the weights `√(y_p/y_q)`
//! do not depend on θ, and neither does `P`. Only `A = K − ω²M(ε) + jωC(σ)`
//! does, so
//!
//! ```text
//! ∂S_pq = −√(y_p/y_q) · λ_pᵀ Pᴴ (∂A) P x_q,        A_rᵀ λ_p = h_p.
//! ```
//!
//! The adjoint `λ_p` is **not** the forward field: `A_r(k_t)` is not
//! complex-symmetric, `A_r(k_t)ᵀ = A_r(−k_t)` with the orders mirrored. It
//! is [`FactoredFloquetCell::back_solve_transpose`] on the **same LU** as
//! the forward solve, so there is exactly one factorization per
//! `(ω, k_t)`, the forward-only count
//! ([`FloquetCell::factorization_count`]). The test side of the
//! contraction is conjugated by the Bloch reduction, so the full-space
//! adjoint field is `conj(P) λ_p = conj(P conj λ_p)`, which is a
//! prolongation at `−k_t`. With `k_t = 0` this collapses to the forward
//! solve (`A_r` is then symmetric); that is why the mutation tripwires
//! ([`FloquetSensitivityFault`]) only bite at oblique incidence.
//!
//! # Parameters
//!
//! * **Material** ([`MaterialDesign`]): `ε = ε′ − jε″` per region,
//!   `∂A/∂ε′_R = −ω²M_R`, `∂A/∂ε″_R = +jω²M_R`. A region may not contain a
//!   tet that holds a port face: the port medium is a separate, fixed
//!   `ε_r` ([`FloquetSensitivityError::MaterialTouchesPort`]).
//! * **Shape** ([`ShapeDesign`]): node-motion velocity columns `∂X/∂θ_i`.
//!   `∂A/∂X` is the exact forward-mode dual-number Nédélec element kernel of
//!   [`crate::driven::shape`] (`∂K + (−ω²ε + jωσ) ∂M`), with per-tet `ε`
//!   and `σ` held fixed as the mesh morphs. PEC nodes may move. Two typed
//!   errors fence the rest, never a silently wrong gradient (the #804
//!   rule):
//!   - a column that moves a node of a **port face** or of a **periodic
//!     face** ([`FloquetSensitivityError::ShapeMovesPinnedNode`]);
//!   - a column that gives paired nodes different motions
//!     ([`FloquetSensitivityError::ShapeBreaksPairing`]; make a velocity
//!     periodic-preserving with
//!     [`PeriodicMap::alias_node_motion`](crate::mesh::periodic::PeriodicMap::alias_node_motion)).
//!
//! # VJP
//!
//! For a real `g(S)` with Wirtinger cotangent `C_pq = ∂g/∂S_pq`,
//! `dg/dθ = −2 Re Σ_q Λ_qᵀ (∂A) R_q` with `Λ_q = Σ_p C_pq √(y_p/y_q) L_p`
//! the folded adjoint, so the cost is `n_p` contractions and not `n_p²`.
//!
//! # Scope
//!
//! p=1, direct LU, one `(ω, k_t)` per call, scalar per-tet `ε` (complex
//! allowed) and optional `σ`, as the forward. Moving port or periodic
//! faces is a later phase.

use faer::c64;

use crate::driven::floquet::{
    FactoredFloquetCell, FloquetCell, FloquetChannel, FloquetError, FloquetSolution,
};
use crate::driven::s_sensitivity::{MaterialDesign, SParam, ShapeDesign};
use crate::driven::shape::{Dual, nedelec_local_dual};
use crate::mesh::TetMesh;
use crate::mesh::periodic::PeriodicMap;

/// Errors of the Floquet S sensitivity.
#[derive(Debug, thiserror::Error)]
pub enum FloquetSensitivityError {
    /// An error of the underlying Floquet forward (assembly, factorization,
    /// channel set, solve).
    #[error(transparent)]
    Floquet(#[from] FloquetError),
    /// The geometry, design or cotangent is inconsistent with the cell.
    #[error("invalid Floquet sensitivity input: {0}")]
    InvalidDesign(String),
    /// A shape column moves a node of a port face or of a periodic face.
    /// This phase pins both (the Floquet port term and the Bloch
    /// prolongation are θ-independent only then).
    #[error(
        "shape column {column} moves node {node} of a {what}; port faces and periodic faces \
         are pinned in this phase (Epic #1252 Phase 1). Zero the velocity on those nodes"
    )]
    ShapeMovesPinnedNode {
        /// Column index.
        column: usize,
        /// The node.
        node: usize,
        /// `"Floquet port face"` or `"periodic face"`.
        what: &'static str,
    },
    /// A shape column moves two paired periodic nodes differently, so the
    /// perturbed mesh would no longer be periodic.
    #[error(
        "shape column {column} moves node {node} differently from its periodic master \
         {master}; the motion must be periodic-preserving (PeriodicMap::alias_node_motion)"
    )]
    ShapeBreaksPairing {
        /// Column index.
        column: usize,
        /// The aliased node.
        node: usize,
        /// Its canonical master.
        master: usize,
    },
    /// A material region contains a tet that holds a face of a Floquet
    /// port. The port medium is the port's fixed real `ε_r`.
    #[error(
        "material region {region} contains tet {tet}, which holds a face of Floquet port \
         {port}; the port medium is fixed"
    )]
    MaterialTouchesPort {
        /// Region index.
        region: usize,
        /// The tet.
        tet: usize,
        /// The port.
        port: usize,
    },
}

/// Test-only fault injection: the mutation tripwires of issue #1256. Each
/// replaces the Hermitian-pairing-aware adjoint by the plain mistake it
/// guards against. All of them are exact at normal incidence (`k_t = 0`),
/// where `A_r` is symmetric, and must fail the FD check at oblique
/// incidence on a cell without a mirror symmetry.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloquetSensitivityFault {
    /// The correct adjoint.
    #[default]
    None,
    /// `λ_p = A_r⁻¹ h_p`: a forward solve instead of the mirrored-order
    /// transpose solve (the complex-symmetric shortcut of #842).
    ForwardSolveAsAdjoint,
    /// The right transpose solve, but the full-space adjoint is `P λ_p`
    /// where it must be `conj(P) λ_p` (the Bloch phase is not conjugated
    /// on the test side).
    UnconjugatedPhase,
}

/// Options of the Floquet S sensitivity.
#[derive(Debug, Clone, Copy, Default)]
pub struct FloquetSensitivityOptions {
    /// Test-only fault injection. Never set it in production.
    #[doc(hidden)]
    pub fault: FloquetSensitivityFault,
}

/// The geometry and materials a [`FloquetCell`] was assembled from. The
/// cell keeps only its operator, so the sensitivity needs them again, and
/// they must be the ones the cell was assembled with.
#[derive(Debug, Clone, Copy)]
pub struct FloquetGeometry<'a> {
    /// The mesh.
    pub mesh: &'a TetMesh,
    /// The periodic map the cell was assembled with.
    pub map: &'a PeriodicMap,
    /// Complex relative permittivity per tet
    /// ([`FloquetCellSpec::eps_r`](crate::driven::floquet::FloquetCellSpec::eps_r)).
    pub eps_r: &'a [c64],
    /// Optional conductivity per tet
    /// ([`FloquetCellSpec::sigma`](crate::driven::floquet::FloquetCellSpec::sigma)).
    pub sigma: Option<&'a [f64]>,
}

/// The design parameters. At least one of the two must be set.
#[derive(Debug, Clone, Default)]
pub struct FloquetDesign {
    /// Per-region `ε′` / `ε″` (`None`: no material parameters).
    pub material: Option<MaterialDesign>,
    /// Node-motion columns (`None`: no shape parameters).
    pub shape: Option<ShapeDesign>,
}

/// Result of [`floquet_s_sensitivity`].
#[derive(Debug, Clone)]
pub struct FloquetSensitivity {
    /// Angular frequency.
    pub omega: f64,
    /// Transverse wave vector.
    pub k_t: [f64; 3],
    /// Every channel of the forward (propagating and evanescent).
    pub channels: Vec<FloquetChannel>,
    /// Indices into [`Self::channels`] of the propagating channels: the
    /// rows and columns of [`Self::s`] and of every [`Self::ds`] table.
    pub propagating: Vec<usize>,
    /// The S matrix, `s[i][j]` (outgoing `i`, incident `j`).
    pub s: Vec<Vec<c64>>,
    /// The parameters, in table order: every region's `ε′` (if selected),
    /// then every region's `ε″` (if selected), then the shape columns.
    pub params: Vec<SParam>,
    /// `ds[param][i][j] = ∂S_ij/∂θ_param`.
    pub ds: Vec<Vec<Vec<c64>>>,
    /// Sparse LU factorizations this call performed (1: the adjoint shares
    /// the forward LU).
    pub n_factorizations: usize,
    /// Transpose back-substitutions on that LU (one per propagating
    /// channel, none for the VJP's folded adjoints beyond that).
    pub n_adjoint_solves: usize,
    /// Largest relative residual of the forward columns.
    pub residual_rel: f64,
    /// The forward's warnings.
    pub warnings: Vec<String>,
}

impl FloquetSensitivity {
    /// `dg/dθ = 2 Re Σ_ij C_ij ∂S_ij/∂θ` for a Wirtinger cotangent
    /// `cotangents[i][j] = ∂g/∂S_ij` of a real `g(S)`: the JVP-side
    /// contraction [`floquet_s_vjp`] is checked against.
    ///
    /// # Panics
    ///
    /// Panics if `cotangents` does not match the S matrix shape.
    pub fn contract(&self, cotangents: &[Vec<c64>]) -> Vec<f64> {
        let np = self.propagating.len();
        assert_eq!(cotangents.len(), np, "cotangent rows");
        self.ds
            .iter()
            .map(|table| {
                let mut acc = c64::new(0.0, 0.0);
                for i in 0..np {
                    assert_eq!(cotangents[i].len(), np, "cotangent columns");
                    for j in 0..np {
                        acc += cotangents[i][j] * table[i][j];
                    }
                }
                2.0 * acc.re
            })
            .collect()
    }
}

/// Result of [`floquet_s_vjp`].
#[derive(Debug, Clone)]
pub struct FloquetVjp {
    /// The objective `g(S)`.
    pub objective: f64,
    /// `dg/dθ`, in [`Self::params`] order.
    pub grad: Vec<f64>,
    /// The parameters.
    pub params: Vec<SParam>,
    /// The S matrix.
    pub s: Vec<Vec<c64>>,
    /// Sparse LU factorizations this call performed (1).
    pub n_factorizations: usize,
    /// Transpose back-substitutions on that LU.
    pub n_adjoint_solves: usize,
}

/// `∂S_ij/∂θ` for every entry of the Floquet S matrix of `cell` at
/// `(omega, k_t)` and every parameter of `design` (module docs: the
/// identity, the parameters, the fences). `phi` is the `κ = 0`
/// polarization azimuth, as [`FloquetCell::factor`].
///
/// # Errors
///
/// [`FloquetSensitivityError::InvalidDesign`] for inputs inconsistent with
/// the cell, [`FloquetSensitivityError::ShapeMovesPinnedNode`],
/// [`FloquetSensitivityError::ShapeBreaksPairing`] and
/// [`FloquetSensitivityError::MaterialTouchesPort`] for the fenced
/// designs, and [`FloquetSensitivityError::Floquet`] for a forward failure.
pub fn floquet_s_sensitivity(
    cell: &FloquetCell,
    geo: &FloquetGeometry<'_>,
    omega: f64,
    k_t: [f64; 3],
    phi: f64,
    design: &FloquetDesign,
    opts: &FloquetSensitivityOptions,
) -> Result<FloquetSensitivity, FloquetSensitivityError> {
    let prep = Prepared::new(cell, geo, design)?;
    let n0 = cell.factorization_count();
    let at = prep.solve_at(omega, k_t, phi, opts)?;
    let np = at.sol.propagating.len();

    // Pairs (L_i, R_j), row-major.
    let mut lefts: Vec<&[c64]> = Vec::with_capacity(np * np);
    let mut rights: Vec<&[c64]> = Vec::with_capacity(np * np);
    for i in 0..np {
        for j in 0..np {
            lefts.push(&at.adjoints[i]);
            rights.push(&at.sol.fields[j]);
        }
    }
    let t = prep.volume_terms(omega, &lefts, &rights);
    let ds: Vec<Vec<Vec<c64>>> = t
        .iter()
        .map(|per_pair| {
            (0..np)
                .map(|i| {
                    (0..np)
                        .map(|j| -per_pair[i * np + j] * at.weight[i][j])
                        .collect()
                })
                .collect()
        })
        .collect();
    Ok(FloquetSensitivity {
        omega,
        k_t,
        channels: at.sol.channels.clone(),
        propagating: at.sol.propagating.clone(),
        s: at.sol.s.clone(),
        params: prep.params.clone(),
        ds,
        n_factorizations: cell.factorization_count() - n0,
        n_adjoint_solves: np,
        residual_rel: at.sol.residual_rel,
        warnings: at.sol.warnings.clone(),
    })
}

/// The gradient `dg/dθ` of a real `g(S)` through the same identity as
/// [`floquet_s_sensitivity`], folded into one adjoint per incident
/// channel (module docs).
///
/// `objective(s)` receives the S matrix (`s[i][j]`) and returns
/// `(g, ∂g/∂S)` with the cotangent the **Wirtinger** derivative
/// `∂g/∂S_ij` (un-conjugated; e.g. `S̄_ij` for `|S_ij|²`).
///
/// # Errors
///
/// As [`floquet_s_sensitivity`], plus
/// [`FloquetSensitivityError::InvalidDesign`] for a cotangent of the wrong
/// shape.
#[allow(clippy::too_many_arguments)]
pub fn floquet_s_vjp<G>(
    cell: &FloquetCell,
    geo: &FloquetGeometry<'_>,
    omega: f64,
    k_t: [f64; 3],
    phi: f64,
    design: &FloquetDesign,
    opts: &FloquetSensitivityOptions,
    objective: G,
) -> Result<FloquetVjp, FloquetSensitivityError>
where
    G: Fn(&[Vec<c64>]) -> (f64, Vec<Vec<c64>>),
{
    let prep = Prepared::new(cell, geo, design)?;
    let n0 = cell.factorization_count();
    let at = prep.solve_at(omega, k_t, phi, opts)?;
    let np = at.sol.propagating.len();
    let (g, cot) = objective(&at.sol.s);
    if cot.len() != np || cot.iter().any(|r| r.len() != np) {
        return Err(FloquetSensitivityError::InvalidDesign(format!(
            "objective cotangent must be {np}×{np}"
        )));
    }
    let n = at.adjoints.first().map_or(0, Vec::len);
    // Λ_j = Σ_i C_ij √(y_i/y_j) L_i.
    let folded: Vec<Vec<c64>> = (0..np)
        .map(|j| {
            let mut v = vec![c64::new(0.0, 0.0); n];
            for (i, adj) in at.adjoints.iter().enumerate() {
                let f = cot[i][j] * at.weight[i][j];
                if f == c64::new(0.0, 0.0) {
                    continue;
                }
                for (o, &l) in v.iter_mut().zip(adj) {
                    *o += f * l;
                }
            }
            v
        })
        .collect();
    let lefts: Vec<&[c64]> = folded.iter().map(Vec::as_slice).collect();
    let rights: Vec<&[c64]> = at.sol.fields.iter().map(Vec::as_slice).collect();
    let t = prep.volume_terms(omega, &lefts, &rights);
    let grad: Vec<f64> = t
        .iter()
        .map(|per_q| {
            let mut acc = c64::new(0.0, 0.0);
            for &v in per_q {
                acc += v;
            }
            -2.0 * acc.re
        })
        .collect();
    Ok(FloquetVjp {
        objective: g,
        grad,
        params: prep.params.clone(),
        s: at.sol.s.clone(),
        n_factorizations: cell.factorization_count() - n0,
        n_adjoint_solves: np,
    })
}

/// Shape design data: per-node "moves" flags.
struct ShapeData<'a> {
    design: &'a ShapeDesign,
    moving: Vec<bool>,
}

struct Prepared<'a> {
    cell: &'a FloquetCell,
    geo: &'a FloquetGeometry<'a>,
    material: Option<&'a MaterialDesign>,
    shape: Option<ShapeData<'a>>,
    params: Vec<SParam>,
    tet_idx: Vec<[u32; 6]>,
    tet_sign: Vec<[i8; 6]>,
}

/// The forward and adjoint fields at one `(ω, k_t)`.
struct At {
    sol: FloquetSolution,
    /// Full-space adjoint field `conj(P) λ_p` per propagating channel.
    adjoints: Vec<Vec<c64>>,
    /// `√(y_p/y_q)`, `weight[p][q]`.
    weight: Vec<Vec<f64>>,
}

impl<'a> Prepared<'a> {
    fn new(
        cell: &'a FloquetCell,
        geo: &'a FloquetGeometry<'a>,
        design: &'a FloquetDesign,
    ) -> Result<Self, FloquetSensitivityError> {
        let invalid = |m: String| FloquetSensitivityError::InvalidDesign(m);
        let mesh = geo.mesh;
        let n_tets = mesh.n_tets();
        let edges = mesh.edges();
        if cell.operator().n_dofs() != edges.len() {
            return Err(invalid(format!(
                "the cell has {} edge DOFs, the mesh {} edges: it was assembled on another mesh",
                cell.operator().n_dofs(),
                edges.len()
            )));
        }
        if !geo.map.matches_mesh(mesh) {
            return Err(invalid("the periodic map was built on another mesh".into()));
        }
        if geo.eps_r.len() != n_tets {
            return Err(invalid(format!(
                "eps_r has {} entries, the mesh {n_tets} tets",
                geo.eps_r.len()
            )));
        }
        if let Some(s) = geo.sigma
            && s.len() != n_tets
        {
            return Err(invalid(format!(
                "sigma has {} entries, the mesh {n_tets} tets",
                s.len()
            )));
        }
        if design.material.is_none() && design.shape.is_none() {
            return Err(invalid("the design has no parameters".into()));
        }
        let tet_edges = mesh.tet_edges();
        let tet_idx: Vec<[u32; 6]> = tet_edges
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect();
        let tet_sign: Vec<[i8; 6]> = tet_edges
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect();

        let mut params = Vec::new();
        if let Some(m) = &design.material {
            if m.region_of_tet().len() != n_tets {
                return Err(invalid(format!(
                    "the material design labels {} tets, the mesh has {n_tets}",
                    m.region_of_tet().len()
                )));
            }
            for port in 0..2 {
                let dofs = cell.port_dofs(port);
                for (t, r) in m.region_of_tet().iter().enumerate() {
                    let Some(region) = *r else { continue };
                    let on_face = tet_idx[t]
                        .iter()
                        .filter(|&&e| dofs.binary_search(&(e as usize)).is_ok())
                        .count();
                    if on_face >= 3 {
                        return Err(FloquetSensitivityError::MaterialTouchesPort {
                            region,
                            tet: t,
                            port,
                        });
                    }
                }
            }
            if m.eps_prime {
                params.extend((0..m.n_regions()).map(|region| SParam::EpsPrime { region }));
            }
            if m.eps_dprime {
                params.extend((0..m.n_regions()).map(|region| SParam::EpsDoublePrime { region }));
            }
        }
        let shape = match &design.shape {
            Some(sh) => {
                let moving = check_shape(cell, geo, sh, &edges)?;
                params.extend((0..sh.n_columns()).map(|column| SParam::Shape { column }));
                Some(ShapeData { design: sh, moving })
            }
            None => None,
        };
        if params.is_empty() {
            return Err(invalid("the design selects no parameters".into()));
        }
        Ok(Self {
            cell,
            geo,
            material: design.material.as_ref(),
            shape,
            params,
            tet_idx,
            tet_sign,
        })
    }

    /// Factor once, solve every forward column and every adjoint on that LU.
    fn solve_at(
        &self,
        omega: f64,
        k_t: [f64; 3],
        phi: f64,
        opts: &FloquetSensitivityOptions,
    ) -> Result<At, FloquetSensitivityError> {
        let fac: FactoredFloquetCell<'_> = self.cell.factor(omega, k_t, phi)?;
        let sol = fac.s_matrix()?;
        let np = sol.propagating.len();
        let n = fac.n_reduced();
        let zero = c64::new(0.0, 0.0);
        let mut adjoints = Vec::with_capacity(np);
        for &c in &sol.propagating {
            let h = fac.port_vector(c);
            let mut lam = vec![zero; n];
            match opts.fault {
                FloquetSensitivityFault::ForwardSolveAsAdjoint => fac.back_solve(&h, &mut lam)?,
                _ => fac.back_solve_transpose(&h, &mut lam),
            }
            let full = if opts.fault == FloquetSensitivityFault::UnconjugatedPhase {
                fac.expand(&lam)
            } else {
                // conj(P) λ = conj(P conj λ).
                let conj: Vec<c64> = lam.iter().map(|z| z.conj()).collect();
                fac.expand(&conj).into_iter().map(|z| z.conj()).collect()
            };
            adjoints.push(full);
        }
        let y: Vec<f64> = sol
            .propagating
            .iter()
            .map(|&c| sol.channels[c].admittance.re)
            .collect();
        let weight = (0..np)
            .map(|p| (0..np).map(|q| (y[p] / y[q]).sqrt()).collect())
            .collect();
        Ok(At {
            sol,
            adjoints,
            weight,
        })
    }

    /// `t[param][pair] = l_pairᵀ (∂A/∂θ_param) r_pair` on full edge vectors.
    fn volume_terms(&self, omega: f64, lefts: &[&[c64]], rights: &[&[c64]]) -> Vec<Vec<c64>> {
        let zero = c64::new(0.0, 0.0);
        let n_pairs = lefts.len();
        let omega2 = omega * omega;
        let mesh = self.geo.mesh;
        let mut out = vec![vec![zero; n_pairs]; self.params.len()];
        let gather = |v: &[c64], t: usize| -> [c64; 6] {
            std::array::from_fn(|i| v[self.tet_idx[t][i] as usize] * f64::from(self.tet_sign[t][i]))
        };

        // Material: z_R = Σ_{t ∈ R} lᵀ M_t r.
        if let Some(m) = self.material {
            let mut z = vec![vec![zero; n_pairs]; m.n_regions()];
            for (t, r) in m.region_of_tet().iter().enumerate() {
                let Some(region) = *r else { continue };
                let tet = &mesh.tets[t];
                let base: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
                let (_, mm, _) = nedelec_local_dual(&base.map(|v| v.map(Dual::cst)));
                for q in 0..n_pairs {
                    let (l, r) = (gather(lefts[q], t), gather(rights[q], t));
                    let mut acc = zero;
                    for i in 0..6 {
                        for j in 0..6 {
                            acc += l[i] * r[j] * mm[i][j].re;
                        }
                    }
                    z[region][q] += acc;
                }
            }
            for (i, prm) in self.params.iter().enumerate() {
                match *prm {
                    SParam::EpsPrime { region } => {
                        for (o, &zz) in out[i].iter_mut().zip(&z[region]) {
                            *o = zz * (-omega2);
                        }
                    }
                    SParam::EpsDoublePrime { region } => {
                        for (o, &zz) in out[i].iter_mut().zip(&z[region]) {
                            *o = zz * c64::new(0.0, omega2);
                        }
                    }
                    SParam::Shape { .. } => {}
                }
            }
        }

        // Shape: Σ_t Σ_(a,axis) V[node][axis] · l_locᵀ ∂A_loc r_loc.
        if let Some(sh) = &self.shape {
            let first_shape = self
                .params
                .iter()
                .position(|p| matches!(p, SParam::Shape { .. }))
                .expect("shape data implies shape params");
            let iomega = c64::new(0.0, omega);
            for (t, tet) in mesh.tets.iter().enumerate() {
                if !tet.iter().any(|&nd| sh.moving[nd as usize]) {
                    continue;
                }
                let l_loc: Vec<[c64; 6]> = lefts.iter().map(|l| gather(l, t)).collect();
                let r_loc: Vec<[c64; 6]> = rights.iter().map(|r| gather(r, t)).collect();
                let base: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
                let sig_t = self.geo.sigma.map_or(0.0, |s| s[t]);
                let mass_coeff = self.geo.eps_r[t] * (-omega2) + iomega * sig_t;
                for a in 0..4 {
                    let node = tet[a] as usize;
                    if !sh.moving[node] {
                        continue;
                    }
                    for axis in 0..3 {
                        if (0..sh.design.n_columns())
                            .all(|c| sh.design.column(c)[node][axis] == 0.0)
                        {
                            continue;
                        }
                        let mut dc = base.map(|v| v.map(Dual::cst));
                        dc[a][axis] = Dual::var(base[a][axis]);
                        let (dk, dm, _) = nedelec_local_dual(&dc);
                        let da: [[c64; 6]; 6] = std::array::from_fn(|i| {
                            std::array::from_fn(|j| {
                                c64::new(dk[i][j].du, 0.0) + mass_coeff * dm[i][j].du
                            })
                        });
                        let vals: Vec<c64> = (0..n_pairs)
                            .map(|q| {
                                let (l, r) = (&l_loc[q], &r_loc[q]);
                                let mut acc = zero;
                                for i in 0..6 {
                                    let mut row = zero;
                                    for j in 0..6 {
                                        row += da[i][j] * r[j];
                                    }
                                    acc += l[i] * row;
                                }
                                acc
                            })
                            .collect();
                        for c in 0..sh.design.n_columns() {
                            let v = sh.design.column(c)[node][axis];
                            if v == 0.0 {
                                continue;
                            }
                            for (o, &val) in out[first_shape + c].iter_mut().zip(&vals) {
                                *o += val * v;
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

/// Validate the shape columns against the pinned faces and the periodic
/// pairing. Returns the per-node "moves in some column" flags.
fn check_shape(
    cell: &FloquetCell,
    geo: &FloquetGeometry<'_>,
    sh: &ShapeDesign,
    edges: &[[u32; 2]],
) -> Result<Vec<bool>, FloquetSensitivityError> {
    let n_nodes = geo.mesh.n_nodes();
    for c in 0..sh.n_columns() {
        if sh.column(c).len() != n_nodes {
            return Err(FloquetSensitivityError::InvalidDesign(format!(
                "shape column {c} has {} nodes, the mesh {n_nodes}",
                sh.column(c).len()
            )));
        }
    }
    let mut on_port = vec![false; n_nodes];
    for p in 0..2 {
        for &d in cell.port_dofs(p) {
            for &nd in &edges[d] {
                on_port[nd as usize] = true;
            }
        }
    }
    let mut moving = vec![false; n_nodes];
    for c in 0..sh.n_columns() {
        let col = sh.column(c);
        let scale = col.iter().flatten().fold(1.0_f64, |m, &v| m.max(v.abs()));
        let tol = 1e-12 * scale;
        let differs = |a: [f64; 3], b: [f64; 3]| (0..3).any(|k| (a[k] - b[k]).abs() > tol);
        // Pairing first: the more specific diagnosis.
        for (i, v) in col.iter().enumerate() {
            if let Some(al) = geo.map.node_alias(i)
                && al.master != i
                && differs(*v, col[al.master])
            {
                return Err(FloquetSensitivityError::ShapeBreaksPairing {
                    column: c,
                    node: i,
                    master: al.master,
                });
            }
        }
        for (i, v) in col.iter().enumerate() {
            if v.iter().all(|x| x.abs() <= tol) {
                continue;
            }
            if on_port[i] {
                return Err(FloquetSensitivityError::ShapeMovesPinnedNode {
                    column: c,
                    node: i,
                    what: "Floquet port face",
                });
            }
            if geo.map.node_alias(i).is_some() {
                return Err(FloquetSensitivityError::ShapeMovesPinnedNode {
                    column: c,
                    node: i,
                    what: "periodic face",
                });
            }
        }
        for (i, v) in col.iter().enumerate() {
            if v.iter().any(|&x| x != 0.0) {
                moving[i] = true;
            }
        }
    }
    Ok(moving)
}
