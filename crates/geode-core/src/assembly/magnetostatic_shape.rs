//! **Discrete shape adjoint** of the nonlinear sector torque with respect to
//! the mesh node coordinates `X` (motor-paper milestone M3, part B, issue
//! #1174).
//!
//! The forward model is the one of
//! [`crate::assembly::magnetostatic_sector`]: the P1 nonlinear
//! magnetostatic residual of
//! [`crate::assembly::magnetostatic_nonlinear`] on a [`SlidingBand`], either
//! with the conforming band or with the harmonic coupling
//! ([`CouplingMode`]), at a rotor step `k`. This module differentiates the
//! **discrete** torque `T(X) = T(X, A(X), λ(X))` exactly, so that a Taylor
//! test `|T(X + hV) − T(X) − h ⟨dT/dX, V⟩| = O(h²)` holds for any smooth node
//! velocity `V` that the forward model accepts.
//!
//! # Element shape derivatives
//!
//! On a P1 triangle with barycentric gradients `g_p` and unsigned area
//! `|T|`, moving node `q` by `δx_q` gives, exactly to first order,
//!
//! ```text
//!   ∂|T|/∂x_q = |T| g_q ,      ∂g_p/∂x_{q,c} = −g_{p,c} g_q ,
//! ```
//!
//! (the affine map's `δ|T| = |T| div V` and `δ∇λ = −(∇V)ᵀ ∇λ` for the P1
//! velocity `V = Σ_q δx_q λ_q`). Both use the **true** gradients and the
//! **unsigned** area, as every assembler term since #1166/#1167 does, so the
//! derivative does not depend on the triangle orientation. Everything below
//! is built from these two identities; no finite difference enters.
//!
//! # Residual
//!
//! The full nodal residual is `R_p = Σ_T |T| ν(|G|²) g_p·G − F_p`, with
//! `G = Σ_p a_p g_p = ∇A_z`. Its contraction with an adjoint `ψ`
//! (`Ψ = Σ_p ψ_p g_p`) differentiates to the Maxwell-stress-like element
//! tensor
//!
//! ```text
//!   ∂(ψᵀ R_T)/∂x_q = |T| [ ν (Ψ·G) I − ν (G Ψᵀ + Ψ Gᵀ) − 2 ν' (Ψ·G) G Gᵀ ] g_q
//!                    − ∂(ψᵀ F_T)/∂x_q ,
//! ```
//!
//! with `ν' = dν/d(b²)`. The load `F` has three parts:
//!
//! * **Current.** A winding group `g` carries a prescribed total current
//!   `I_g` (ampere-turns, signed), so `J_g = I_g / A_g(X)` with
//!   `A_g = Σ_{T∈g} |T|` (the `icem_forward` rule `J = I·N·sign/area_group`
//!   with the mesh area, [`GroupAreaRule::MovesWithMesh`]). When a slot
//!   grows, its current density falls. `ψᵀF_g = I_g Q_g / A_g` with
//!   `Q_g = Σ_{T∈g} |T| Σ_p ψ_p / 3`, so each triangle of the group
//!   contributes `J_g (Σ_p ψ_p/3 − Q_g/A_g) ∂|T|/∂x_q`.
//!   [`GroupAreaRule::Fixed`] keeps the areas (and so `J`) fixed instead.
//! * **Magnetization.** `F_p = |T| g_p·w` with `w = (−M_y, M_x)`
//!   (the `∫ M·(∇φ)^⊥` load of
//!   [`NonlinearMagnetostaticSystem::with_magnetization`](crate::assembly::magnetostatic_nonlinear::NonlinearMagnetostaticSystem::with_magnetization));
//!   `M` is a per-triangle constant that does not depend on `X` (the
//!   magnet orientation is a material property).
//! * Any Dirichlet value is fixed (`A_D` does not depend on `X`), and the
//!   node links and harmonic constraints are topological or live on the
//!   coupling circle, which may not move.
//!
//! # Torque functionals
//!
//! * [`ShapeTorque::Arkkio`]: `T = Σ_bands L/(μ₀(r_o − r_i)) Σ_{T∈band}
//!   |T| (B·c)(B·c^⊥)/|c|`, the [`crate::assembly::torque::arkkio_torque`]
//!   sum written with the centroid `c` (`r B_r B_θ = (B·c)(B·c^⊥)/|c|`). It
//!   depends on `X` explicitly through `|T|`, `B` and `c`; those terms are
//!   included, so a velocity that moves band nodes off the coupling circle
//!   is handled exactly too.
//! * [`ShapeTorque::Coupling`]: the harmonic coupling torque
//!   `T = −L λᵀ D A` of [`SlidingBand::harmonic_coupling_torque`]
//!   (`D = ∂(Gᵀ A)/∂β`). `D` lives on the coupling circle, so `T` has no
//!   explicit `X` dependence; it depends on the multipliers `λ`.
//!
//! # Adjoint
//!
//! With reduced unknowns `a` (`A = P a + A_D`) the state equations are
//! `R(a, X) + C λ = 0`, `Cᵀ a + d = 0` (`C = Pᵀ G`; `C` is empty for the
//! conforming band). The converged saddle matrix `[[K, C], [Cᵀ, 0]]` is
//! symmetric, so one solve
//!
//! ```text
//!   [ K   C ] [ ψ ]     [ Pᵀ ∂T/∂A ]
//!   [ Cᵀ  0 ] [ χ ] = − [ ∂T/∂λ    ] ,
//! ```
//!
//! gives `dT/dX = ∂T/∂X + (P ψ)ᵀ ∂R/∂X`. With the issue's adjoint
//! `Kᵀ μ = ∂T/∂A`, `ψ = −μ` and this is `∂T/∂X − μᵀ ∂R/∂X`. `K` is the
//! converged consistent tangent the forward solve returns
//! ([`NonlinearMagnetostaticSolution::tangent`](crate::assembly::magnetostatic_nonlinear::NonlinearMagnetostaticSolution::tangent));
//! the multiplier block is eliminated through the dense `m × m` Schur
//! complement, as in
//! [`NonlinearMagnetostaticSystem::solve_constrained`](crate::assembly::magnetostatic_nonlinear::NonlinearMagnetostaticSystem::solve_constrained).
//! There is one adjoint per rotor angle and current.
//!
//! **Rotation.** At step `k` the rotor nodes sit at `R_k X`. The element
//! terms are evaluated on that rotated mesh, and the rotor rows of the
//! gradient are rotated back with `R_kᵀ`, so every gradient here is with
//! respect to the **reference** (`k = 0`) coordinates of the split mesh,
//! which is what a design velocity `∂X/∂θ` is given on.
//!
//! # Objectives
//!
//! [`AngleSetGradient`] carries the gradients of the mean torque over an
//! angle set and of its **population** standard deviation (the reference
//! ripple objective, `benchmarks/motor_reference/conventions.json`
//! `torque.ripple_definition`): `dσ = Σ_j (T_j − T̄) dT_j / (n σ)`.
//! [`design_gradient`] turns a node gradient into
//! `dJ/dθ_k = ⟨dJ/dX, V_k⟩` for the node velocities `V_k = ∂X/∂θ_k` of a
//! design parametrization.
//!
//! The validation is in three places. The unit tests of this module check
//! the element terms against central differences. `tests/motor_shape_gradient.rs`
//! runs CI-fast Taylor tests on the ICEM sector. `benchmarks/motor_shape_gradient/`
//! holds the full study: Taylor tests in both coupling modes, the
//! central-difference step sweep, the tolerance study and the cost of an
//! adjoint.

use std::time::Instant;

use faer::Mat;
use faer::linalg::solvers::Solve;

use crate::analytic::slotless_pm::MU_0;
use crate::analytic::waveguide::{TriMesh, tri_bary_grads};
use crate::assembly::magnetostatic_nonlinear::{
    MaterialMap, NewtonConfig, NonlinearMagnetostaticError, ReluctivityLaw,
};
use crate::assembly::magnetostatic_sector::{
    CouplingMode, SectorError, SlidingBand, SlidingBandProblem, SlidingBandSolution,
};

/// One Arkkio band: the triangles carrying `tag`, normalised by the annulus
/// `[r_inner, r_outer]` (see [`crate::assembly::torque::arkkio_torque`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArkkioTerm {
    /// Triangle tag of the band.
    pub tag: i32,
    /// Inner radius of the normalising annulus (m).
    pub r_inner: f64,
    /// Outer radius of the normalising annulus (m).
    pub r_outer: f64,
}

/// The torque functional to differentiate.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeTorque {
    /// The sum of the Arkkio torques of `terms` (N m with `l_axial` in m).
    Arkkio {
        /// The bands.
        terms: Vec<ArkkioTerm>,
        /// Axial length (m).
        l_axial: f64,
    },
    /// The harmonic coupling torque `−L λᵀ ∂(Gᵀ A)/∂β` (only with a
    /// harmonic [`CouplingMode`]).
    Coupling {
        /// Axial length (m).
        l_axial: f64,
    },
}

/// How the winding-group areas behind `J = I / A_g` depend on the design.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupAreaRule {
    /// `A_g(X) = Σ_{T∈g} |T|`, the mesh area at the current node positions:
    /// the total current of a group is prescribed and its density follows
    /// the slot area (the `icem_forward` `AreaMode::Mesh` rule).
    MovesWithMesh,
    /// Fixed areas (m², one per group): the current density does not
    /// depend on `X`.
    Fixed(Vec<f64>),
}

/// One rotor angle and current: the band step `k` and the signed total
/// current of each winding group (A, turns included).
#[derive(Debug, Clone, PartialEq)]
pub struct OperatingPoint {
    /// Sliding-band step (rotor angle `kΔ`).
    pub k: i64,
    /// Total current per winding group (A·turns, signed).
    pub group_currents: Vec<f64>,
}

/// A nonlinear sector problem whose torque is differentiated with respect
/// to the node coordinates (module docs).
#[derive(Debug, Clone)]
pub struct SectorShapeProblem {
    band: SlidingBand,
    materials: MaterialMap,
    magnetization: Option<Vec<[f64; 2]>>,
    /// Dirichlet mask on the **input** mesh (as [`SlidingBandProblem::new`]).
    dirichlet: Vec<bool>,
    coupling: CouplingMode,
    groups: Vec<Option<usize>>,
    n_groups: usize,
    area_rule: GroupAreaRule,
}

/// A converged forward solve at one operating point.
#[derive(Debug, Clone)]
pub struct ShapeSolve {
    /// The operating point.
    pub op: OperatingPoint,
    /// The sliding-band solve (state, multipliers, converged tangent).
    pub solution: SlidingBandSolution,
    /// The torque functional (N m).
    pub torque: f64,
    /// Wall time of the forward solve (s).
    pub forward_seconds: f64,
}

/// `dT/dX` at one operating point.
#[derive(Debug, Clone)]
pub struct TorqueShapeGradient {
    /// The torque (N m).
    pub torque: f64,
    /// `dT/dX` per split-mesh node, reference frame (N m / m).
    pub grad: Vec<[f64; 2]>,
    /// The explicit part `∂T/∂X` alone (zero unless the functional reads
    /// moving geometry), reference frame.
    pub explicit: Vec<[f64; 2]>,
    /// Wall time of the adjoint solve and the gradient assembly (s).
    pub adjoint_seconds: f64,
}

/// Torques and gradients over an angle set at one current, with the mean
/// and the population standard deviation.
#[derive(Debug, Clone)]
pub struct AngleSetGradient {
    /// Torque per operating point (N m).
    pub torques: Vec<f64>,
    /// `dT_j/dX` per operating point.
    pub torque_grads: Vec<Vec<[f64; 2]>>,
    /// Mean torque.
    pub mean: f64,
    /// `d(mean)/dX`.
    pub mean_grad: Vec<[f64; 2]>,
    /// Population standard deviation of the torque.
    pub std: f64,
    /// `d(std)/dX`.
    pub std_grad: Vec<[f64; 2]>,
    /// Forward wall time per operating point (s).
    pub forward_seconds: Vec<f64>,
    /// Adjoint wall time per operating point (s).
    pub adjoint_seconds: Vec<f64>,
}

fn invalid(s: String) -> SectorError {
    SectorError::InvalidInput(s)
}

impl SectorShapeProblem {
    /// Set up the problem on `band` (reference position, `k = 0`).
    ///
    /// * `groups[t]` is the winding group of triangle `t` (`None` carries no
    ///   current); groups are numbered `0..n_groups`.
    /// * `dirichlet` is the per-node mask on the **input** mesh.
    ///
    /// The current density of group `g` is `I_g / A_g` with the
    /// [`GroupAreaRule::MovesWithMesh`] areas unless
    /// [`with_fixed_group_areas`](Self::with_fixed_group_areas) is used.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] on a length mismatch or a group with no
    /// triangle.
    pub fn new(
        band: SlidingBand,
        materials: MaterialMap,
        groups: Vec<Option<usize>>,
        dirichlet: &[bool],
    ) -> Result<Self, SectorError> {
        if groups.len() != band.mesh().n_tris() {
            return Err(invalid(format!(
                "{} group entries for {} triangles",
                groups.len(),
                band.mesh().n_tris()
            )));
        }
        let n_groups = groups.iter().flatten().map(|&g| g + 1).max().unwrap_or(0);
        let mut seen = vec![false; n_groups];
        for &g in groups.iter().flatten() {
            seen[g] = true;
        }
        if let Some(g) = seen.iter().position(|s| !s) {
            return Err(invalid(format!("winding group {g} has no triangle")));
        }
        if dirichlet.len() != band.original_nodes().len() - count_split(&band) {
            return Err(invalid(format!(
                "dirichlet mask length {} != input node count {}",
                dirichlet.len(),
                band.original_nodes().len() - count_split(&band)
            )));
        }
        Ok(Self {
            band,
            materials,
            magnetization: None,
            dirichlet: dirichlet.to_vec(),
            coupling: CouplingMode::Conforming,
            groups,
            n_groups,
            area_rule: GroupAreaRule::MovesWithMesh,
        })
    }

    /// Add a per-triangle magnetization (A/m) at the reference position.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] on a length mismatch.
    pub fn with_magnetization(mut self, m: &[[f64; 2]]) -> Result<Self, SectorError> {
        if m.len() != self.band.mesh().n_tris() {
            return Err(invalid(format!(
                "magnetization length {} != triangle count {}",
                m.len(),
                self.band.mesh().n_tris()
            )));
        }
        self.magnetization = Some(m.to_vec());
        Ok(self)
    }

    /// Choose the coupling (as [`SlidingBandProblem::with_coupling`]).
    ///
    /// # Errors
    ///
    /// As [`SlidingBand::check_harmonic_orders`].
    pub fn with_coupling(mut self, mode: CouplingMode) -> Result<Self, SectorError> {
        // Validate through the forward problem's own checks.
        let probe = SlidingBandProblem::new(
            self.band.clone(),
            self.materials.clone(),
            &vec![0.0; self.band.mesh().n_tris()],
            &self.dirichlet,
        )?
        .with_coupling(mode)?;
        self.coupling = probe.coupling().clone();
        Ok(self)
    }

    /// Keep the winding-group areas fixed at `areas` (m²) instead of the
    /// mesh areas ([`GroupAreaRule::Fixed`]).
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] unless there is one positive finite
    /// area per group.
    pub fn with_fixed_group_areas(mut self, areas: Vec<f64>) -> Result<Self, SectorError> {
        if areas.len() != self.n_groups || areas.iter().any(|a| !(a.is_finite() && *a > 0.0)) {
            return Err(invalid(format!(
                "need {} positive group areas, got {:?}",
                self.n_groups, areas
            )));
        }
        self.area_rule = GroupAreaRule::Fixed(areas);
        Ok(self)
    }

    /// The band (reference position).
    pub fn band(&self) -> &SlidingBand {
        &self.band
    }

    /// The coupling mode.
    pub fn coupling(&self) -> &CouplingMode {
        &self.coupling
    }

    /// Number of winding groups.
    pub fn n_groups(&self) -> usize {
        self.n_groups
    }

    /// The split-mesh node positions at the reference position.
    pub fn nodes(&self) -> &[[f64; 2]] {
        &self.band.mesh().nodes
    }

    /// The same problem with the split-mesh nodes moved to `nodes` (see
    /// [`SlidingBand::with_node_positions`]).
    ///
    /// # Errors
    ///
    /// As [`SlidingBand::with_node_positions`].
    pub fn with_node_positions(&self, nodes: Vec<[f64; 2]>) -> Result<Self, SectorError> {
        let mut out = self.clone();
        out.band = self.band.with_node_positions(nodes)?;
        Ok(out)
    }

    /// The design `X + h V` for a split-mesh velocity `v`.
    ///
    /// # Errors
    ///
    /// As [`with_node_positions`](Self::with_node_positions), and
    /// [`SectorError::InvalidInput`] on a length mismatch.
    pub fn displaced(&self, v: &[[f64; 2]], h: f64) -> Result<Self, SectorError> {
        let x = self.nodes();
        if v.len() != x.len() {
            return Err(invalid(format!(
                "velocity length {} != split-mesh node count {}",
                v.len(),
                x.len()
            )));
        }
        self.with_node_positions(
            x.iter()
                .zip(v)
                .map(|(p, d)| [p[0] + h * d[0], p[1] + h * d[1]])
                .collect(),
        )
    }

    /// Check that a split-mesh velocity is admissible: finite, and exactly
    /// zero on the coupling circle (which the forward model cannot move).
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] otherwise.
    pub fn check_velocity(&self, v: &[[f64; 2]]) -> Result<(), SectorError> {
        if v.len() != self.nodes().len() {
            return Err(invalid(format!(
                "velocity length {} != split-mesh node count {}",
                v.len(),
                self.nodes().len()
            )));
        }
        if let Some(i) = v
            .iter()
            .position(|d| !(d[0].is_finite() && d[1].is_finite()))
        {
            return Err(invalid(format!("velocity at node {i} is not finite")));
        }
        for c in self.band.coupling_nodes() {
            let d = v[c as usize];
            if d[0] != 0.0 || d[1] != 0.0 {
                return Err(invalid(format!(
                    "velocity at coupling-circle node {c} is ({:e}, {:e}); it must be zero",
                    d[0], d[1]
                )));
            }
        }
        Ok(())
    }

    /// Winding-group areas `A_g` (m²) in use: the mesh areas at the current
    /// node positions, or the fixed ones.
    pub fn group_areas(&self) -> Vec<f64> {
        match &self.area_rule {
            GroupAreaRule::Fixed(a) => a.clone(),
            GroupAreaRule::MovesWithMesh => {
                let mesh = self.band.mesh();
                let mut a = vec![0.0; self.n_groups];
                for (t, g) in self.groups.iter().enumerate() {
                    if let Some(g) = g {
                        let [p, q, r] = mesh.tris[t].map(|i| mesh.nodes[i as usize]);
                        let s =
                            0.5 * ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]));
                        a[*g] += s.abs();
                    }
                }
                a
            }
        }
    }

    /// Per-triangle current density `J_t = I_g / A_g` (A/m²).
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] unless there is one current per group.
    pub fn current_density(&self, group_currents: &[f64]) -> Result<Vec<f64>, SectorError> {
        if group_currents.len() != self.n_groups {
            return Err(invalid(format!(
                "{} group currents for {} groups",
                group_currents.len(),
                self.n_groups
            )));
        }
        let areas = self.group_areas();
        let jg: Vec<f64> = group_currents
            .iter()
            .zip(&areas)
            .map(|(i, a)| i / a)
            .collect();
        Ok(self
            .groups
            .iter()
            .map(|g| g.map_or(0.0, |g| jg[g]))
            .collect())
    }

    /// The forward [`SlidingBandProblem`] for the given group currents.
    ///
    /// # Errors
    ///
    /// As [`SlidingBandProblem::new`] and
    /// [`current_density`](Self::current_density).
    pub fn sliding_problem(
        &self,
        group_currents: &[f64],
    ) -> Result<SlidingBandProblem, SectorError> {
        let j = self.current_density(group_currents)?;
        let mut p = SlidingBandProblem::new(
            self.band.clone(),
            self.materials.clone(),
            &j,
            &self.dirichlet,
        )?;
        if let Some(m) = &self.magnetization {
            p = p.with_magnetization(m)?;
        }
        p.with_coupling(self.coupling.clone())
    }

    /// Solve the forward problem at `op` and evaluate `torque`.
    ///
    /// # Errors
    ///
    /// As [`SlidingBandProblem::solve_at`], and
    /// [`SectorError::InvalidInput`] for [`ShapeTorque::Coupling`] with the
    /// conforming band.
    pub fn solve(
        &self,
        op: &OperatingPoint,
        torque: &ShapeTorque,
        config: &NewtonConfig,
        initial: Option<&[f64]>,
    ) -> Result<ShapeSolve, SectorError> {
        self.check_torque(torque)?;
        let t0 = Instant::now();
        let p = self.sliding_problem(&op.group_currents)?;
        let s = p.solve_at(op.k, config, initial)?;
        let forward_seconds = t0.elapsed().as_secs_f64();
        let value = self.torque_value(&s, torque)?;
        Ok(ShapeSolve {
            op: op.clone(),
            solution: s,
            torque: value,
            forward_seconds,
        })
    }

    fn check_torque(&self, torque: &ShapeTorque) -> Result<(), SectorError> {
        match torque {
            ShapeTorque::Coupling { .. } if self.coupling.harmonic().is_none() => Err(invalid(
                "the coupling torque needs a harmonic coupling mode".into(),
            )),
            ShapeTorque::Arkkio { terms, .. } => {
                if let Some(t) = terms.iter().find(|t| {
                    t.r_outer.partial_cmp(&t.r_inner) != Some(std::cmp::Ordering::Greater)
                }) {
                    return Err(invalid(format!("Arkkio band {t:?}: r_outer <= r_inner")));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The torque functional of a converged solve (N m).
    fn torque_value(
        &self,
        s: &SlidingBandSolution,
        torque: &ShapeTorque,
    ) -> Result<f64, SectorError> {
        Ok(match torque {
            ShapeTorque::Arkkio { terms, l_axial } => {
                arkkio_with_derivatives(
                    &s.mesh,
                    self.band.tri_tags(),
                    &s.solution.a_z,
                    terms,
                    *l_axial,
                    false,
                )
                .0
            }
            ShapeTorque::Coupling { l_axial } => {
                let (orders, free) = self
                    .coupling
                    .harmonic()
                    .ok_or_else(|| invalid("coupling torque without harmonic coupling".into()))?;
                let rows = self.band.harmonic_coupling_torque_rows(s.k, orders, free)?;
                -l_axial * coupling_contract(&rows, &s.solution.a_z, &s.multipliers)
            }
        })
    }

    /// `dT/dX` at a converged solve `s` of this design (module docs).
    ///
    /// # Errors
    ///
    /// [`SectorError::Solve`] if the tangent factorization fails or the
    /// Schur complement is singular; [`SectorError::InvalidInput`] if `s`
    /// does not belong to this problem.
    pub fn torque_gradient(
        &self,
        s: &ShapeSolve,
        torque: &ShapeTorque,
    ) -> Result<TorqueShapeGradient, SectorError> {
        self.check_torque(torque)?;
        let t0 = Instant::now();
        let sol = &s.solution;
        let k = sol.k;
        let n_nodes = self.band.mesh().n_nodes();
        let a = &sol.solution.a_z;
        if a.len() != n_nodes || sol.mesh.n_nodes() != n_nodes {
            return Err(invalid("solve does not belong to this problem".into()));
        }
        let mesh_k = &sol.mesh;
        let tags = self.band.tri_tags();

        // ∂T/∂A (full nodal), ∂T/∂λ, explicit ∂T/∂X (rotated frame).
        let (value, dt_da, dt_dlam, mut explicit) = match torque {
            ShapeTorque::Arkkio { terms, l_axial } => {
                let (v, da, dx) = arkkio_with_derivatives(mesh_k, tags, a, terms, *l_axial, true);
                (v, da, vec![0.0; sol.multipliers.len()], dx)
            }
            ShapeTorque::Coupling { l_axial } => {
                let (orders, free) = self
                    .coupling
                    .harmonic()
                    .ok_or_else(|| invalid("coupling torque without harmonic coupling".into()))?;
                let rows = self.band.harmonic_coupling_torque_rows(k, orders, free)?;
                if rows.len() != sol.multipliers.len() {
                    return Err(invalid(
                        "multiplier count does not match the coupling".into(),
                    ));
                }
                let mut da = vec![0.0; n_nodes];
                let mut dl = vec![0.0; rows.len()];
                for (j, row) in rows.iter().enumerate() {
                    for &(node, w) in row {
                        da[node] -= l_axial * sol.multipliers[j] * w;
                        dl[j] -= l_axial * w * a[node];
                    }
                }
                let v = -l_axial * coupling_contract(&rows, a, &sol.multipliers);
                (v, da, dl, vec![[0.0; 2]; n_nodes])
            }
        };

        // Adjoint: [[K, C], [Cᵀ, 0]] [ψ; χ] = −[Pᵀ ∂T/∂A; ∂T/∂λ].
        let f: Vec<f64> = sol.solution.reduce(&dt_da).iter().map(|x| -x).collect();
        let psi_red = match self.coupling.harmonic() {
            None => sol.solution.solve_tangent_transpose(&f)?,
            Some((orders, free)) => {
                let g_neg: Vec<f64> = dt_dlam.iter().map(|x| -x).collect();
                let p = self.sliding_problem(&s.op.group_currents)?;
                let (_, sys) = p.system_at(k)?;
                let g = self.band.harmonic_constraints(k, orders, free)?;
                let (c, _d) = sys.reduced_constraints(&g)?;
                saddle_solve(&sol.solution.tangent, &c, &f, &g_neg)?
            }
        };
        // ψ_full = P ψ.
        let psi: Vec<f64> = sol
            .solution
            .free_of_global
            .iter()
            .zip(&sol.solution.sign_of_global)
            .map(|(slot, sg)| slot.map_or(0.0, |fi| sg * psi_red[fi]))
            .collect();

        // ψᵀ ∂R/∂X on the rotated mesh.
        let j_tri = self.current_density(&s.op.group_currents)?;
        let m_k = self
            .magnetization
            .as_ref()
            .map(|m| self.band.rotate_rotor_vectors(k, m));
        let mut grad = vec![[0.0; 2]; n_nodes];
        residual_shape_contraction(
            &ResidualData {
                mesh: mesh_k,
                tags,
                materials: &self.materials,
                groups: &self.groups,
                n_groups: self.n_groups,
                fixed_areas: matches!(self.area_rule, GroupAreaRule::Fixed(_)),
                j_tri: &j_tri,
                magnetization: m_k.as_deref(),
            },
            a,
            &psi,
            &mut grad,
        )?;
        for (g, e) in grad.iter_mut().zip(&explicit) {
            g[0] += e[0];
            g[1] += e[1];
        }

        // Rotate rotor rows back to the reference frame.
        let th = self.band.angle_of(k);
        let (cs, sn) = (th.cos(), th.sin());
        let back = |v: [f64; 2]| [cs * v[0] + sn * v[1], -sn * v[0] + cs * v[1]];
        for (i, &r) in self.band.rotor_nodes().iter().enumerate() {
            if r {
                grad[i] = back(grad[i]);
                explicit[i] = back(explicit[i]);
            }
        }
        Ok(TorqueShapeGradient {
            torque: value,
            grad,
            explicit,
            adjoint_seconds: t0.elapsed().as_secs_f64(),
        })
    }

    /// Solve every operating point in order (each warm-started from
    /// `initials[j]` if given, else from the previous point's state) and
    /// return the solves.
    ///
    /// # Errors
    ///
    /// The first [`solve`](Self::solve) error, or
    /// [`SectorError::InvalidInput`] if `initials` has the wrong length.
    pub fn solve_angle_set(
        &self,
        ops: &[OperatingPoint],
        torque: &ShapeTorque,
        config: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<Vec<ShapeSolve>, SectorError> {
        if let Some(init) = initials
            && init.len() != ops.len()
        {
            return Err(invalid(format!(
                "{} initial guesses for {} operating points",
                init.len(),
                ops.len()
            )));
        }
        let mut out: Vec<ShapeSolve> = Vec::with_capacity(ops.len());
        for (j, op) in ops.iter().enumerate() {
            let prev = match initials {
                Some(init) => Some(init[j].clone()),
                None => out.last().map(|s| s.solution.solution.a_z.clone()),
            };
            out.push(self.solve(op, torque, config, prev.as_deref())?);
        }
        Ok(out)
    }

    /// Torque gradients over an angle set and the gradients of their mean
    /// and population standard deviation (module docs).
    ///
    /// # Errors
    ///
    /// As [`torque_gradient`](Self::torque_gradient);
    /// [`SectorError::InvalidInput`] for an empty set.
    pub fn angle_set_gradient(
        &self,
        solves: &[ShapeSolve],
        torque: &ShapeTorque,
    ) -> Result<AngleSetGradient, SectorError> {
        if solves.is_empty() {
            return Err(invalid("empty angle set".into()));
        }
        let mut torques = Vec::with_capacity(solves.len());
        let mut grads = Vec::with_capacity(solves.len());
        let mut adjoint_seconds = Vec::with_capacity(solves.len());
        for s in solves {
            let g = self.torque_gradient(s, torque)?;
            torques.push(g.torque);
            grads.push(g.grad);
            adjoint_seconds.push(g.adjoint_seconds);
        }
        let (mean, mean_grad, std, std_grad) = mean_std_gradients(&torques, &grads);
        Ok(AngleSetGradient {
            torques,
            torque_grads: grads,
            mean,
            mean_grad,
            std,
            std_grad,
            forward_seconds: solves.iter().map(|s| s.forward_seconds).collect(),
            adjoint_seconds,
        })
    }

    /// `dJ/dθ_k = ⟨dJ/dX, V_k⟩` for split-mesh velocities `V_k = ∂X/∂θ_k`
    /// (see [`design_gradient`]), after checking each velocity with
    /// [`check_velocity`](Self::check_velocity).
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] for an inadmissible velocity.
    pub fn design_gradient(
        &self,
        grad: &[[f64; 2]],
        velocities: &[Vec<[f64; 2]>],
    ) -> Result<Vec<f64>, SectorError> {
        for v in velocities {
            self.check_velocity(v)?;
        }
        Ok(design_gradient(grad, velocities))
    }

    /// Expand a per-node velocity given on the **input** mesh to the split
    /// mesh (split coupling copies take their original's value).
    ///
    /// # Panics
    ///
    /// Panics if `v` is not one entry per input-mesh node.
    pub fn expand_velocity(&self, v: &[[f64; 2]]) -> Vec<[f64; 2]> {
        self.band.expand_nodal(v)
    }
}

/// The inputs of [`residual_shape_contraction`] besides `A` and `ψ`.
#[derive(Clone, Copy)]
struct ResidualData<'a> {
    mesh: &'a TriMesh,
    tags: &'a [i32],
    materials: &'a MaterialMap,
    groups: &'a [Option<usize>],
    n_groups: usize,
    /// `J` fixed (`true`) or `J_g = I_g / A_g(X)` (`false`).
    fixed_areas: bool,
    j_tri: &'a [f64],
    magnetization: Option<&'a [[f64; 2]]>,
}

/// `ψᵀ ∂R/∂X` for the full nodal residual on `mesh` (the rotated mesh
/// of the solve), accumulated into `out` (module docs).
fn residual_shape_contraction(
    d: &ResidualData<'_>,
    a: &[f64],
    psi: &[f64],
    out: &mut [[f64; 2]],
) -> Result<(), SectorError> {
    let ResidualData {
        mesh,
        tags,
        materials,
        groups,
        n_groups,
        fixed_areas,
        j_tri,
        magnetization,
    } = *d;
    let n_t = mesh.n_tris();
    // Per-triangle geometry.
    let geom: Vec<([[f64; 2]; 3], f64)> = mesh
        .tris
        .iter()
        .map(|tri| {
            let c = tri.map(|i| mesh.nodes[i as usize]);
            let (g, _, _, area) = tri_bary_grads(&c);
            (g, area)
        })
        .collect();
    // Current: coefficient e_t on ∂|T| in ψᵀF.
    let psi_sum = |t: usize| -> f64 { mesh.tris[t].iter().map(|&p| psi[p as usize]).sum() };
    let mut e = vec![0.0_f64; n_t];
    if fixed_areas {
        for t in 0..n_t {
            if j_tri[t] != 0.0 {
                e[t] = j_tri[t] * psi_sum(t) / 3.0;
            }
        }
    } else {
        let mut q = vec![0.0_f64; n_groups];
        let mut area = vec![0.0_f64; n_groups];
        for t in 0..n_t {
            if let Some(g) = groups[t] {
                q[g] += geom[t].1 * psi_sum(t) / 3.0;
                area[g] += geom[t].1;
            }
        }
        for t in 0..n_t {
            if let Some(g) = groups[t] {
                e[t] = j_tri[t] * (psi_sum(t) / 3.0 - q[g] / area[g]);
            }
        }
    }
    for t in 0..n_t {
        let tri = mesh.tris[t];
        let (g, area) = geom[t];
        let law = materials.get(tags[t]).ok_or(SectorError::Solve(
            NonlinearMagnetostaticError::MissingMaterial {
                tag: tags[t],
                triangle: t,
            },
        ))?;
        let mut gr = [0.0; 2];
        let mut ps = [0.0; 2];
        for p in 0..3 {
            let (ap, pp) = (a[tri[p] as usize], psi[tri[p] as usize]);
            gr[0] += ap * g[p][0];
            gr[1] += ap * g[p][1];
            ps[0] += pp * g[p][0];
            ps[1] += pp * g[p][1];
        }
        let w = magnetization.map_or([0.0; 2], |m| [-m[t][1], m[t][0]]);
        let has_field = ps[0] != 0.0 || ps[1] != 0.0;
        if !has_field && e[t] == 0.0 {
            continue;
        }
        let (nu, two_dnu) = element_law(law.as_ref(), gr);
        let pg = ps[0] * gr[0] + ps[1] * gr[1];
        let pw = ps[0] * w[0] + ps[1] * w[1];
        for q in 0..3 {
            let gq = g[q];
            let gqg = gq[0] * gr[0] + gq[1] * gr[1];
            let pgq = ps[0] * gq[0] + ps[1] * gq[1];
            let gqw = gq[0] * w[0] + gq[1] * w[1];
            let node = tri[q] as usize;
            for c in 0..2 {
                let stiff = nu * pg * gq[c]
                    - nu * gqg * ps[c]
                    - nu * pgq * gr[c]
                    - two_dnu * pg * gqg * gr[c];
                let load_m = gq[c] * pw - ps[c] * gqw;
                let load_j = e[t] * gq[c];
                out[node][c] += area * (stiff - load_m - load_j);
            }
        }
    }
    Ok(())
}

/// Number of split copies in a band (split-mesh nodes minus input nodes).
fn count_split(band: &SlidingBand) -> usize {
    band.original_nodes()
        .iter()
        .enumerate()
        .filter(|(i, o)| **o as usize != *i)
        .count()
}

/// `ν` and `2 dν/d(b²)` at an element gradient.
fn element_law(law: &dyn ReluctivityLaw, gr: [f64; 2]) -> (f64, f64) {
    let b2 = gr[0] * gr[0] + gr[1] * gr[1];
    (law.nu(b2), 2.0 * law.dnu_db2(b2))
}

/// `Σ_j λ_j Σ_(i,w) w A_i`.
fn coupling_contract(rows: &[Vec<(usize, f64)>], a: &[f64], lam: &[f64]) -> f64 {
    rows.iter()
        .zip(lam)
        .map(|(row, l)| l * row.iter().map(|&(n, w)| w * a[n]).sum::<f64>())
        .sum()
}

/// The Arkkio sum of `terms` on `mesh` at the full nodal potential `a`,
/// with (if `derivs`) `∂T/∂A` and the explicit `∂T/∂X` on the same mesh.
fn arkkio_with_derivatives(
    mesh: &TriMesh,
    tags: &[i32],
    a: &[f64],
    terms: &[ArkkioTerm],
    l_axial: f64,
    derivs: bool,
) -> (f64, Vec<f64>, Vec<[f64; 2]>) {
    let n = mesh.n_nodes();
    let (mut da, mut dx) = if derivs {
        (vec![0.0; n], vec![[0.0; 2]; n])
    } else {
        (Vec::new(), Vec::new())
    };
    let mut value = 0.0;
    for term in terms {
        let kk = l_axial / (MU_0 * (term.r_outer - term.r_inner));
        let mut acc = 0.0;
        for (t, tri) in mesh.tris.iter().enumerate() {
            if tags[t] != term.tag {
                continue;
            }
            let c3 = tri.map(|i| mesh.nodes[i as usize]);
            let (g, _, _, area) = tri_bary_grads(&c3);
            let mut gr = [0.0; 2];
            for p in 0..3 {
                let ap = a[tri[p] as usize];
                gr[0] += ap * g[p][0];
                gr[1] += ap * g[p][1];
            }
            let b = [gr[1], -gr[0]];
            let c = [
                (c3[0][0] + c3[1][0] + c3[2][0]) / 3.0,
                (c3[0][1] + c3[1][1] + c3[2][1]) / 3.0,
            ];
            let rc = c[0].hypot(c[1]);
            let cp = [-c[1], c[0]];
            let bc = b[0] * c[0] + b[1] * c[1];
            let bcp = b[0] * cp[0] + b[1] * cp[1];
            let f = bc * bcp / rc;
            acc += area * f;
            if !derivs {
                continue;
            }
            // ∂f/∂B and ∂f/∂c.
            let fb = [
                (c[0] * bcp + cp[0] * bc) / rc,
                (c[1] * bcp + cp[1] * bc) / rc,
            ];
            let fc = [
                (b[0] * bcp + b[1] * bc) / rc - f * c[0] / (rc * rc),
                (b[1] * bcp - b[0] * bc) / rc - f * c[1] / (rc * rc),
            ];
            for p in 0..3 {
                // ∂B/∂a_p = (g_p,y, −g_p,x).
                da[tri[p] as usize] += kk * area * (fb[0] * g[p][1] - fb[1] * g[p][0]);
            }
            for q in 0..3 {
                let node = tri[q] as usize;
                // ∂B/∂x_{q,c} = −G_c (g_q,y, −g_q,x).
                let fb_dir = fb[0] * g[q][1] - fb[1] * g[q][0];
                for comp in 0..2 {
                    dx[node][comp] +=
                        kk * area * (g[q][comp] * f - gr[comp] * fb_dir + fc[comp] / 3.0);
                }
            }
        }
        value += kk * acc;
    }
    (value, da, dx)
}

/// Solve `[[K, C], [Cᵀ, 0]] [ψ; χ] = [f; g]` for `ψ` (`K` sparse and
/// symmetric, `C` dense `n × m`) through the Schur complement `Cᵀ K⁻¹ C`.
fn saddle_solve(
    k: &faer::sparse::SparseColMat<usize, f64>,
    c: &Mat<f64>,
    f: &[f64],
    g: &[f64],
) -> Result<Vec<f64>, SectorError> {
    let n = f.len();
    let m = g.len();
    if c.nrows() != n || c.ncols() != m {
        return Err(invalid(format!(
            "constraint matrix {} x {} for {n} unknowns and {m} multipliers",
            c.nrows(),
            c.ncols()
        )));
    }
    let lu = k.as_ref().sp_lu().map_err(|e| {
        SectorError::Solve(NonlinearMagnetostaticError::Factorization(format!("{e:?}")))
    })?;
    let mut rhs: Mat<f64> =
        Mat::from_fn(n, m + 1, |i, j| if j == 0 { f[i] } else { c[(i, j - 1)] });
    lu.solve_in_place(rhs.as_mut());
    if m == 0 {
        return Ok((0..n).map(|i| rhs[(i, 0)]).collect());
    }
    // S = Cᵀ Y, r = Cᵀ z − g, χ = S⁻¹ r, ψ = z − Y χ.
    let s = Mat::<f64>::from_fn(m, m, |i, j| {
        (0..n).map(|q| c[(q, i)] * rhs[(q, j + 1)]).sum()
    });
    let r = Mat::<f64>::from_fn(m, 1, |i, _| {
        (0..n).map(|q| c[(q, i)] * rhs[(q, 0)]).sum::<f64>() - g[i]
    });
    let chi = s.partial_piv_lu().solve(&r);
    if (0..m).any(|i| !chi[(i, 0)].is_finite()) {
        return Err(SectorError::Solve(NonlinearMagnetostaticError::NonFinite(
            "adjoint Schur complement solve".into(),
        )));
    }
    Ok((0..n)
        .map(|i| rhs[(i, 0)] - (0..m).map(|j| rhs[(i, j + 1)] * chi[(j, 0)]).sum::<f64>())
        .collect())
}

/// Mean and population standard deviation of `t`, with their gradients
/// from the per-point gradients `grads` (module docs).
///
/// # Panics
///
/// Panics if `t` is empty or the lengths differ.
pub fn mean_std_gradients(
    t: &[f64],
    grads: &[Vec<[f64; 2]>],
) -> (f64, Vec<[f64; 2]>, f64, Vec<[f64; 2]>) {
    assert!(
        !t.is_empty() && t.len() == grads.len(),
        "mean_std_gradients: lengths"
    );
    let n = t.len() as f64;
    let n_nodes = grads[0].len();
    let mean = t.iter().sum::<f64>() / n;
    let var = t.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let std = var.sqrt();
    let mut mg = vec![[0.0; 2]; n_nodes];
    let mut sg = vec![[0.0; 2]; n_nodes];
    for (tj, gj) in t.iter().zip(grads) {
        let ws = if std > 0.0 {
            (tj - mean) / (n * std)
        } else {
            0.0
        };
        for i in 0..n_nodes {
            for c in 0..2 {
                mg[i][c] += gj[i][c] / n;
                sg[i][c] += ws * gj[i][c];
            }
        }
    }
    (mean, mg, std, sg)
}

/// `⟨grad, v⟩ = Σ_i grad_i · v_i`.
///
/// # Panics
///
/// Panics on a length mismatch.
pub fn directional(grad: &[[f64; 2]], v: &[[f64; 2]]) -> f64 {
    assert_eq!(grad.len(), v.len(), "directional: lengths");
    grad.iter()
        .zip(v)
        .map(|(g, d)| g[0] * d[0] + g[1] * d[1])
        .sum()
}

/// `dJ/dθ_k = ⟨dJ/dX, V_k⟩` for node velocities `V_k = ∂X/∂θ_k` on the
/// split mesh (reference position), ready for a parametrization's
/// `∂X/∂θ`. The velocities must vanish on the coupling circle
/// ([`SectorShapeProblem::design_gradient`] checks this).
///
/// # Panics
///
/// Panics on a length mismatch.
pub fn design_gradient(grad: &[[f64; 2]], velocities: &[Vec<[f64; 2]>]) -> Vec<f64> {
    velocities.iter().map(|v| directional(grad, v)).collect()
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)] // index loops over nodes and components in the FD checks
mod tests {
    use super::*;
    use crate::assembly::magnetostatic_nonlinear::{BrauerLaw, ConstantLaw};

    fn tri_mesh() -> TriMesh {
        // A small irregular patch.
        TriMesh {
            nodes: vec![
                [0.0, 0.0],
                [1.0, 0.1],
                [2.1, 0.0],
                [0.1, 1.0],
                [1.2, 1.1],
                [2.0, 0.9],
                [0.0, 2.0],
                [0.9, 2.1],
                [2.2, 2.0],
            ],
            tris: vec![
                [0, 1, 4],
                [0, 4, 3],
                [1, 2, 5],
                [1, 5, 4],
                [3, 4, 7],
                [3, 7, 6],
                [4, 5, 8],
                [4, 8, 7],
            ],
        }
    }

    #[test]
    fn arkkio_derivatives_match_finite_differences() {
        let mesh = tri_mesh();
        let tags = vec![4; mesh.n_tris()];
        let a: Vec<f64> = (0..mesh.n_nodes())
            .map(|i| (0.3 * i as f64).sin() + 0.1 * i as f64)
            .collect();
        let terms = [ArkkioTerm {
            tag: 4,
            r_inner: 0.5,
            r_outer: 2.5,
        }];
        let shifted = |m: &TriMesh| {
            let mut m = m.clone();
            for p in m.nodes.iter_mut() {
                p[0] += 3.0;
                p[1] += 1.0;
            }
            m
        };
        let mesh = shifted(&mesh);
        let (_, da, dx) = arkkio_with_derivatives(&mesh, &tags, &a, &terms, 0.1, true);
        let h = 1e-6;
        for i in 0..mesh.n_nodes() {
            let mut ap = a.clone();
            let mut am = a.clone();
            ap[i] += h;
            am[i] -= h;
            let fd = (arkkio_with_derivatives(&mesh, &tags, &ap, &terms, 0.1, false).0
                - arkkio_with_derivatives(&mesh, &tags, &am, &terms, 0.1, false).0)
                / (2.0 * h);
            assert!(
                (fd - da[i]).abs() <= 1e-6 * da[i].abs().max(1.0),
                "{fd} {}",
                da[i]
            );
            for c in 0..2 {
                let mut mp = mesh.clone();
                let mut mm = mesh.clone();
                mp.nodes[i][c] += h;
                mm.nodes[i][c] -= h;
                let fd = (arkkio_with_derivatives(&mp, &tags, &a, &terms, 0.1, false).0
                    - arkkio_with_derivatives(&mm, &tags, &a, &terms, 0.1, false).0)
                    / (2.0 * h);
                assert!(
                    (fd - dx[i][c]).abs() <= 1e-6 * dx[i][c].abs().max(1.0),
                    "node {i} comp {c}: fd {fd} vs {}",
                    dx[i][c]
                );
            }
        }
    }

    #[test]
    fn mean_std_gradient_is_consistent() {
        let t = [1.0, 2.0, 4.0];
        let g: Vec<Vec<[f64; 2]>> = vec![vec![[1.0, 0.0]], vec![[0.0, 1.0]], vec![[2.0, -1.0]]];
        let (mean, mg, std, sg) = mean_std_gradients(&t, &g);
        assert!((mean - 7.0 / 3.0).abs() < 1e-15);
        let var: f64 = t.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / 3.0;
        assert!((std - var.sqrt()).abs() < 1e-15);
        // Directional check along x of node 0: dT = (1, 0, 2) per unit.
        let h = 1e-6;
        let tp: Vec<f64> = t
            .iter()
            .zip([1.0, 0.0, 2.0])
            .map(|(x, d)| x + h * d)
            .collect();
        let tm: Vec<f64> = t
            .iter()
            .zip([1.0, 0.0, 2.0])
            .map(|(x, d)| x - h * d)
            .collect();
        let s = |v: &[f64]| {
            let m = v.iter().sum::<f64>() / 3.0;
            (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / 3.0).sqrt()
        };
        assert!(((s(&tp) - s(&tm)) / (2.0 * h) - sg[0][0]).abs() < 1e-8);
        assert!((mg[0][0] - 1.0).abs() < 1e-15);
    }

    /// `ψᵀ R(X)` evaluated directly, with `J_g = I_g / A_g(X)` recomputed
    /// on `mesh` unless the areas are fixed.
    fn psi_dot_residual(
        d: &ResidualData<'_>,
        group_currents: &[f64],
        fixed_j: &[f64],
        a: &[f64],
        psi: &[f64],
    ) -> f64 {
        let mesh = d.mesh;
        let mut area_g = vec![0.0; d.n_groups];
        for (t, tri) in mesh.tris.iter().enumerate() {
            if let Some(g) = d.groups[t] {
                let c = tri.map(|i| mesh.nodes[i as usize]);
                area_g[g] += tri_bary_grads(&c).3;
            }
        }
        let mut out = 0.0;
        for (t, tri) in mesh.tris.iter().enumerate() {
            let c = tri.map(|i| mesh.nodes[i as usize]);
            let (g, _, _, area) = tri_bary_grads(&c);
            let (mut gr, mut ps, mut psum) = ([0.0; 2], [0.0; 2], 0.0);
            for p in 0..3 {
                let (ap, pp) = (a[tri[p] as usize], psi[tri[p] as usize]);
                gr[0] += ap * g[p][0];
                gr[1] += ap * g[p][1];
                ps[0] += pp * g[p][0];
                ps[1] += pp * g[p][1];
                psum += pp;
            }
            let law = d.materials.get(d.tags[t]).unwrap();
            let nu = law.nu(gr[0] * gr[0] + gr[1] * gr[1]);
            out += area * nu * (ps[0] * gr[0] + ps[1] * gr[1]);
            let j = if d.fixed_areas {
                fixed_j[t]
            } else {
                d.groups[t].map_or(0.0, |g| group_currents[g] / area_g[g])
            };
            out -= j * area / 3.0 * psum;
            if let Some(m) = d.magnetization {
                let w = [-m[t][1], m[t][0]];
                out -= area * (ps[0] * w[0] + ps[1] * w[1]);
            }
        }
        out
    }

    #[test]
    fn residual_shape_contraction_matches_finite_differences() {
        let mesh = tri_mesh();
        let tags = vec![1, 1, 2, 2, 1, 3, 2, 1];
        let materials = MaterialMap::new()
            .with(1, BrauerLaw::new(0.3, 2.0, 1.1).unwrap())
            .with(2, ConstantLaw::new(1.7).unwrap())
            .with(3, ConstantLaw::new(0.9).unwrap());
        let groups = vec![None, None, Some(0), Some(0), None, None, Some(1), None];
        let currents = [2.5, -1.3];
        let m: Vec<[f64; 2]> = (0..mesh.n_tris())
            .map(|t| {
                if t == 4 || t == 5 {
                    [0.4, -0.7]
                } else {
                    [0.0, 0.0]
                }
            })
            .collect();
        let a: Vec<f64> = (0..mesh.n_nodes())
            .map(|i| 0.8 * (0.7 * i as f64).sin() + 0.2 * i as f64)
            .collect();
        let psi: Vec<f64> = (0..mesh.n_nodes())
            .map(|i| (1.3 * i as f64 + 0.4).cos())
            .collect();
        for fixed_areas in [false, true] {
            // J at the unperturbed mesh (used as is when the areas are fixed).
            let mut area_g = [0.0; 2];
            for (t, tri) in mesh.tris.iter().enumerate() {
                if let Some(g) = groups[t] {
                    area_g[g] += tri_bary_grads(&tri.map(|i| mesh.nodes[i as usize])).3;
                }
            }
            let j_tri: Vec<f64> = groups
                .iter()
                .map(|g| g.map_or(0.0, |g| currents[g] / area_g[g]))
                .collect();
            let mut grad = vec![[0.0; 2]; mesh.n_nodes()];
            let d0 = ResidualData {
                mesh: &mesh,
                tags: &tags,
                materials: &materials,
                groups: &groups,
                n_groups: 2,
                fixed_areas,
                j_tri: &j_tri,
                magnetization: Some(&m),
            };
            residual_shape_contraction(&d0, &a, &psi, &mut grad).unwrap();
            let h = 1e-6;
            for i in 0..mesh.n_nodes() {
                for c in 0..2 {
                    let eval = |s: f64| {
                        let mut mm = mesh.clone();
                        mm.nodes[i][c] += s;
                        let d = ResidualData { mesh: &mm, ..d0 };
                        psi_dot_residual(&d, &currents, &j_tri, &a, &psi)
                    };
                    let fd = (eval(h) - eval(-h)) / (2.0 * h);
                    assert!(
                        (fd - grad[i][c]).abs() <= 1e-7 * grad[i][c].abs().max(1.0),
                        "fixed_areas {fixed_areas}, node {i} comp {c}: fd {fd} vs {}",
                        grad[i][c]
                    );
                }
            }
        }
    }
}
