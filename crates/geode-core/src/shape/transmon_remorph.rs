//! **Per-step re-morphing** and the bounded multi-parameter `C_Σ` design
//! problem on the grounded-transmon fixture (Epic #569 / umbrella #1034,
//! issue #1036, Phase B).
//!
//! Phase A ([`super::transmon_morph`]) built three junction-pinned harmonic
//! fields `D_i(X⁰)` and measured each one's mesh-validity budget along the
//! straight line `X⁰ + θ D_i(X⁰)`. This module lets an optimizer rebuild the
//! fields about the current coordinates instead:
//!
//! ```text
//!   X_{k+1} = X_k + Σ_i Δθ_i · D_i(X_k),
//! ```
//!
//! with the Dirichlet data of each parameter evaluated from `X_k` (for
//! `theta_L`, `(0, y_k − y_pin, 0)`), and optionally a volume-stiffened
//! Laplace operator whose per-tet diffusivity is `(V₀/V_k)^α`.
//! [`RemorphMode`] selects the three variants the issue measures: fixed
//! fields, unit re-morph, stiffened re-morph.
//!
//! # Factorization reuse
//!
//! Fields share one Laplace factorization only when they pin the same node
//! set (prescribed plus fixed-zero). [`MorphFamily::fields`] groups the
//! parameters by that set and factors **once per distinct set**: on the
//! transmon `theta_L` and `theta_W` share one LU (both prescribe the island
//! body and fix the same nodes), and `theta_G`, which frees a ground band
//! that the other two pin, gets its own. Pinning the union instead would
//! change `theta_G`'s field (its band could no longer move), so it is not
//! done. A re-morph step therefore costs two Laplace factorizations for the
//! three fields.
//!
//! # Re-morphing makes `θ` a trace
//!
//! With re-morphing the accumulated `θ_i` is a path integral, not a
//! coordinate: two paths to the same accumulated `θ` give different
//! geometries. The authoritative design output is the physical geometry
//! measured from the final mesh ([`TransmonDimensions`]).
//!
//! # Phase C hooks (#1037, not implemented here)
//!
//! A fourth parameter (the claw gap) is one more entry of a [`MorphFamily`]
//! built with [`MorphFamily::new`]: its fixed set and its Dirichlet-data
//! closure. It joins whichever factorization group has the same pinned set.
//! A second target (a hold on `β = C_g/C_Σ`) is one more row of
//! [`crate::quantum::diffopt::MultiEval`]: `C_g` is another entry of the same
//! [`super::capacitance_matrix_shape_gradient`] once the claw is its own
//! electrode, so it costs no extra factorization. [`TransmonCSigmaProblem`]
//! evaluates the `E_C` row only.

use std::collections::BTreeSet;

use super::transmon_morph::{
    DirectionalBudget, PARAM_NAMES, TransmonMorphRoles, bisect_budget, prescribed_data,
};
use super::{
    apply_node_motion, capacitance_matrix_shape_gradient, harmonic_dirichlet_velocities,
    min_tet_volume_ratio, tet_volume_ratio_derivatives, tet_volume_ratios,
    volume_stiffening_weights, worst_tet_volume_ratio,
};
use crate::assembly::electrostatic::{
    Electrode, ElectrostaticError, assemble_electrostatic, extract_capacitance,
};
use crate::mesh::TetMesh;
use crate::mesh::red_refine::red_refine;
use crate::quantum::diffopt::{MultiEval, MultiParamProblem, StepCheck, StepConstraint};
use crate::quantum::transmon::{d_e_c_hz_d_c_sigma, e_c_hz_from_capacitance};

/// How the parameter fields are rebuilt along a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RemorphMode {
    /// The fields are built once on `X⁰` and never rebuilt (Phase A's map).
    Fixed,
    /// Rebuilt on `X_k` with the unit-coefficient Laplace operator.
    Unit,
    /// Rebuilt on `X_k` with per-tet diffusivity `(V₀/V_k)^alpha`.
    Stiffened {
        /// The stiffening exponent (`0` is the unit operator).
        alpha: f64,
    },
}

impl RemorphMode {
    /// A short label for reports.
    pub fn label(&self) -> String {
        match self {
            Self::Fixed => "fixed".to_string(),
            Self::Unit => "unit_remorph".to_string(),
            Self::Stiffened { alpha } => format!("stiffened_remorph_alpha_{alpha}"),
        }
    }
}

type DataFn = dyn Fn(&TetMesh, usize) -> Vec<(u32, [f64; 3])> + Send + Sync;

/// A set of harmonic morph parameters: per parameter, the nodes its field
/// holds at zero and a closure giving its Dirichlet data on any mesh.
pub struct MorphFamily {
    /// Parameter names, in column order.
    pub names: Vec<String>,
    fixed_zero: Vec<Vec<u32>>,
    data: Box<DataFn>,
}

impl std::fmt::Debug for MorphFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MorphFamily")
            .field("names", &self.names)
            .finish_non_exhaustive()
    }
}

/// Fields of a [`MorphFamily`] and what building them cost.
#[derive(Clone, Debug)]
pub struct FamilyFields {
    /// One field per parameter, in the family's column order.
    pub fields: Vec<Vec<[f64; 3]>>,
    /// Laplace factorizations spent (one per distinct pinned set).
    pub n_factorizations: usize,
    /// The parameter groups that shared a factorization.
    pub groups: Vec<Vec<usize>>,
}

impl MorphFamily {
    /// A custom family: `fixed_zero[i]` is parameter `i`'s fixed set and
    /// `data(mesh, i)` its Dirichlet data evaluated on `mesh`.
    ///
    /// # Panics
    ///
    /// Panics if `names` and `fixed_zero` differ in length.
    pub fn new(
        names: Vec<String>,
        fixed_zero: Vec<Vec<u32>>,
        data: impl Fn(&TetMesh, usize) -> Vec<(u32, [f64; 3])> + Send + Sync + 'static,
    ) -> Self {
        assert_eq!(names.len(), fixed_zero.len(), "one fixed set per parameter");
        let fixed_zero = fixed_zero
            .into_iter()
            .map(|mut f| {
                f.sort_unstable();
                f.dedup();
                f
            })
            .collect();
        Self {
            names,
            fixed_zero,
            data: Box::new(data),
        }
    }

    /// Phase A's three junction-pinned parameters (`theta_L`, `theta_W`,
    /// `theta_G`), with their Dirichlet data from
    /// [`super::transmon_morph::prescribed_data`] and their fixed sets from
    /// [`TransmonMorphRoles::zero_set`].
    pub fn transmon(roles: &TransmonMorphRoles) -> Self {
        let r = roles.clone();
        Self::new(
            PARAM_NAMES.iter().map(|s| (*s).to_string()).collect(),
            (0..3).map(|p| roles.zero_set(p)).collect(),
            move |mesh, p| prescribed_data(mesh, &r, p),
        )
    }

    /// #594's single island-scale parameter: the island moves by `X − c`
    /// (in-plane, `c` the island centroid on the mesh it is evaluated on) and
    /// `fixed_zero` (ground, feedline and far faces) stays put. Its fixed-field
    /// version is [`super::harmonic_extension_velocity`].
    pub fn island_scale(island: Vec<u32>, fixed_zero: Vec<u32>) -> Self {
        let isl = island;
        Self::new(
            vec!["theta_594".to_string()],
            vec![fixed_zero],
            move |mesh, _| {
                let c = super::subset_centroid(mesh, &isl);
                isl.iter()
                    .map(|&n| {
                        let p = mesh.nodes[n as usize];
                        (n, [p[0] - c[0], p[1] - c[1], 0.0])
                    })
                    .collect()
            },
        )
    }

    /// Number of parameters.
    pub fn n_params(&self) -> usize {
        self.names.len()
    }

    /// The Dirichlet data of parameter `p` on `mesh`.
    pub fn data(&self, mesh: &TetMesh, p: usize) -> Vec<(u32, [f64; 3])> {
        (self.data)(mesh, p)
    }

    /// The pinned node set (prescribed ∪ fixed) of parameter `p` on `mesh`,
    /// sorted.
    pub fn pinned_set(&self, mesh: &TetMesh, p: usize) -> Vec<u32> {
        let mut s: BTreeSet<u32> = self.fixed_zero[p].iter().copied().collect();
        s.extend(self.data(mesh, p).iter().map(|(n, _)| *n));
        s.into_iter().collect()
    }

    /// Group `params` by identical pinned set, in first-appearance order.
    pub fn mask_groups(&self, mesh: &TetMesh, params: &[usize]) -> Vec<Vec<usize>> {
        let mut groups: Vec<(Vec<u32>, Vec<usize>)> = Vec::new();
        for &p in params {
            let set = self.pinned_set(mesh, p);
            match groups.iter_mut().find(|(s, _)| *s == set) {
                Some((_, g)) => g.push(p),
                None => groups.push((set, vec![p])),
            }
        }
        groups.into_iter().map(|(_, g)| g).collect()
    }

    /// Build the fields of `params` on `mesh` with the given diffusivity
    /// (`None` = unit), one Laplace factorization per distinct pinned set.
    /// Returns them in the order of `params`.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if a parameter prescribes a node
    /// that is also in its own fixed set (the field would be silently
    /// pinned), else the Laplace assembly / solve errors.
    pub fn fields_of(
        &self,
        mesh: &TetMesh,
        params: &[usize],
        diffusivity: Option<&[f64]>,
    ) -> Result<FamilyFields, ElectrostaticError> {
        let groups = self.mask_groups(mesh, params);
        let mut out: Vec<Option<Vec<[f64; 3]>>> = vec![None; params.len()];
        for group in &groups {
            let datas: Vec<Vec<(u32, [f64; 3])>> =
                group.iter().map(|&p| self.data(mesh, p)).collect();
            for (&p, d) in group.iter().zip(&datas) {
                let fixed: BTreeSet<u32> = self.fixed_zero[p].iter().copied().collect();
                if let Some((n, _)) = d.iter().find(|(n, _)| fixed.contains(n)) {
                    return Err(ElectrostaticError::ShapeMismatch(format!(
                        "{}: prescribed node {n} is also fixed (the field would be silently \
                         pinned)",
                        self.names[p]
                    )));
                }
            }
            // Same pinned set: give every column the union of the prescribed
            // nodes (zero where a column holds the node fixed), and fix only
            // the nodes no column prescribes.
            let union: BTreeSet<u32> = datas.iter().flatten().map(|(n, _)| *n).collect();
            let columns: Vec<Vec<(u32, [f64; 3])>> = datas
                .iter()
                .map(|d| {
                    let mut v: std::collections::BTreeMap<u32, [f64; 3]> =
                        union.iter().map(|&n| (n, [0.0; 3])).collect();
                    for &(n, x) in d {
                        v.insert(n, x);
                    }
                    v.into_iter().collect()
                })
                .collect();
            let fixed: Vec<u32> = self.fixed_zero[group[0]]
                .iter()
                .copied()
                .filter(|n| !union.contains(n))
                .collect();
            let fields = harmonic_dirichlet_velocities(mesh, diffusivity, &columns, &fixed)?;
            for (&p, f) in group.iter().zip(fields) {
                let slot = params.iter().position(|&q| q == p).expect("param in list");
                out[slot] = Some(f);
            }
        }
        Ok(FamilyFields {
            fields: out
                .into_iter()
                .map(|f| f.expect("every parameter built"))
                .collect(),
            n_factorizations: groups.len(),
            groups,
        })
    }

    /// All the family's fields on `current` under `mode` (`base` supplies
    /// the reference volumes of the stiffened operator and, for
    /// [`RemorphMode::Fixed`], the mesh the fields are built on).
    ///
    /// # Errors
    ///
    /// As [`MorphFamily::fields_of`] and [`volume_stiffening_weights`].
    pub fn fields(
        &self,
        base: &TetMesh,
        current: &TetMesh,
        mode: RemorphMode,
    ) -> Result<FamilyFields, ElectrostaticError> {
        let all: Vec<usize> = (0..self.n_params()).collect();
        match mode {
            RemorphMode::Fixed => self.fields_of(base, &all, None),
            RemorphMode::Unit => self.fields_of(current, &all, None),
            RemorphMode::Stiffened { alpha } => {
                let w = volume_stiffening_weights(base, current, alpha)?;
                self.fields_of(current, &all, Some(&w))
            }
        }
    }
}

/// A mesh-validity budget along a re-morphed path in one parameter.
#[derive(Clone, Copy, Debug)]
pub struct RemorphBudget {
    /// The accumulated `θ` at which the worst tet's ratio against `X⁰` meets
    /// the floor (signed).
    pub theta: f64,
    /// `true` if the floor was not reached within the search cap.
    pub unbounded: bool,
    /// Path steps taken (each a field rebuild for the re-morph modes).
    pub n_steps: usize,
    /// Laplace factorizations spent.
    pub n_factorizations: usize,
    /// The worst ratio against `X⁰` at `theta`.
    pub ratio: f64,
    /// Index of the worst tet at `theta`.
    pub worst_tet: usize,
    /// `X⁰` centroid of the worst tet at `theta`.
    pub worst_centroid: [f64; 3],
}

fn tet_centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0_f64; 3];
    for &v in &mesh.tets[t] {
        for (cd, x) in c.iter_mut().zip(mesh.nodes[v as usize]) {
            *cd += 0.25 * x;
        }
    }
    c
}

/// March parameter `param` of `family` in the sign of `sign` from `base`,
/// rebuilding its field every `dtheta` per `mode`, until the worst tet's
/// volume ratio against `base` falls to `ratio_floor`; the last partial step
/// is bisected with its field held fixed. Returns the accumulated `θ` there,
/// and the mesh at that point.
///
/// [`RemorphMode::Fixed`] is the straight line and delegates to
/// [`bisect_budget`] (exact, independent of `dtheta`). The re-morph modes are
/// an explicit-Euler integration of `dX/dθ = D(X)`, so their budget depends on
/// `dtheta`; callers should report it at two step sizes.
///
/// # Errors
///
/// Propagates field-building errors.
///
/// # Panics
///
/// Panics if `sign` or `dtheta` is not positive / nonzero.
#[allow(clippy::too_many_arguments)]
pub fn remorph_budget(
    family: &MorphFamily,
    param: usize,
    base: &TetMesh,
    mode: RemorphMode,
    sign: f64,
    ratio_floor: f64,
    dtheta: f64,
    max_abs_theta: f64,
) -> Result<(RemorphBudget, TetMesh), ElectrostaticError> {
    assert!(sign != 0.0, "remorph_budget: sign must be nonzero");
    assert!(dtheta > 0.0, "remorph_budget: dtheta must be positive");
    let sgn = sign.signum();
    if mode == RemorphMode::Fixed {
        let f = family.fields_of(base, &[param], None)?;
        let b: DirectionalBudget =
            bisect_budget(base, &f.fields[0], ratio_floor, sgn, max_abs_theta);
        let moved = apply_node_motion(base, &f.fields[0], b.theta);
        let (_, worst_tet) = worst_tet_volume_ratio(base, &moved);
        return Ok((
            RemorphBudget {
                theta: b.theta,
                unbounded: b.unbounded,
                n_steps: 0,
                n_factorizations: f.n_factorizations,
                ratio: b.ratio,
                worst_tet,
                worst_centroid: b.worst_centroid,
            },
            moved,
        ));
    }
    let mut x = base.clone();
    let mut acc = 0.0_f64;
    let mut n_steps = 0usize;
    let mut n_fact = 0usize;
    let mut unbounded = false;
    loop {
        let diff = match mode {
            RemorphMode::Stiffened { alpha } => Some(volume_stiffening_weights(base, &x, alpha)?),
            _ => None,
        };
        let f = family.fields_of(&x, &[param], diff.as_deref())?;
        n_fact += f.n_factorizations;
        let d = &f.fields[0];
        let h = dtheta.min(max_abs_theta - acc);
        let trial = apply_node_motion(&x, d, sgn * h);
        n_steps += 1;
        if min_tet_volume_ratio(base, &trial) >= ratio_floor {
            x = trial;
            acc += h;
            if acc >= max_abs_theta {
                unbounded = true;
                break;
            }
            continue;
        }
        let (mut good, mut bad) = (0.0_f64, h);
        for _ in 0..60 {
            let mid = 0.5 * (good + bad);
            if min_tet_volume_ratio(base, &apply_node_motion(&x, d, sgn * mid)) >= ratio_floor {
                good = mid;
            } else {
                bad = mid;
            }
        }
        x = apply_node_motion(&x, d, sgn * good);
        acc += good;
        break;
    }
    let (ratio, t) = worst_tet_volume_ratio(base, &x);
    Ok((
        RemorphBudget {
            theta: sgn * acc,
            unbounded,
            n_steps,
            n_factorizations: n_fact,
            ratio,
            worst_tet: t,
            worst_centroid: tet_centroid(base, t),
        },
        x,
    ))
}

/// The bookkeeping of one evaluated geometry of [`TransmonCSigmaProblem`].
#[derive(Clone, Debug)]
struct Point {
    mesh: TetMesh,
    fields: Vec<Vec<[f64; 3]>>,
    n_laplace_factorizations: usize,
    c_sigma: f64,
    dc_sigma: Vec<f64>,
}

/// What [`TransmonCSigmaProblem`] recorded for one accepted iterate.
#[derive(Clone, Debug)]
pub struct IterateLog {
    /// `C_Σ` (F) from the gradient driver's LU at this iterate.
    pub c_sigma: f64,
    /// `∂C_Σ/∂Δθ_i` (F) at this iterate, along the fields built on it.
    pub dc_sigma: Vec<f64>,
    /// `C_Σ` (F) from the independent fresh extraction, once confirmed.
    pub c_sigma_fresh: Option<f64>,
    /// Laplace factorizations spent building this iterate's fields.
    pub n_laplace_factorizations: usize,
}

/// The transmon `E_C` design problem: Phase A's three parameters, re-morphed
/// per [`RemorphMode`], driven by
/// [`crate::quantum::diffopt::optimize_multiparam_bounded`].
///
/// * **Value:** `E_C/h` (Hz) of the scalar-ε `C_Σ = C_ii − C_if²/C_ff`, from
///   one assembly + one LU ([`capacitance_matrix_shape_gradient`] and its
///   floating reduction).
/// * **Jacobian:** `∂E_C/∂Δθ_i = (dE_C/dC_Σ) · ⟨∇_X C_Σ, D_i(X_k)⟩`.
/// * **Constraint:** every accepted mesh has min tet volume ratio against
///   `X⁰` at least `ratio_floor`; [`MultiParamProblem::check_step`] rejects a
///   trial below it and linearizes its worst tets (target
///   `ratio_floor + margin`).
/// * **Confirmation:** an independent [`assemble_electrostatic`] +
///   [`extract_capacitance`] on the accepted mesh.
pub struct TransmonCSigmaProblem {
    family: MorphFamily,
    mode: RemorphMode,
    base: TetMesh,
    eps_r: Vec<f64>,
    conductors: Vec<Electrode>,
    ground: Vec<u32>,
    ratio_floor: f64,
    margin: f64,
    max_violated: usize,
    current: Point,
    pending: Option<(Vec<f64>, Point)>,
    /// One entry per accepted iterate (index 0 is the start).
    pub log: Vec<IterateLog>,
}

impl std::fmt::Debug for TransmonCSigmaProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransmonCSigmaProblem")
            .field("mode", &self.mode)
            .field("ratio_floor", &self.ratio_floor)
            .field("iterates", &self.log.len())
            .finish_non_exhaustive()
    }
}

impl TransmonCSigmaProblem {
    /// Set up at `base` (`X⁰`). `conductors[0]` is the island and
    /// `conductors[1..]` float (the feedline); `ground` is held at 0 V.
    ///
    /// # Errors
    ///
    /// Propagates field-building and gradient errors at `X⁰`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family: MorphFamily,
        mode: RemorphMode,
        base: TetMesh,
        eps_r: Vec<f64>,
        conductors: Vec<Electrode>,
        ground: Vec<u32>,
        ratio_floor: f64,
        margin: f64,
    ) -> Result<Self, ElectrostaticError> {
        let mut s = Self {
            family,
            mode,
            current: Point {
                mesh: base.clone(),
                fields: Vec::new(),
                n_laplace_factorizations: 0,
                c_sigma: 0.0,
                dc_sigma: Vec::new(),
            },
            base,
            eps_r,
            conductors,
            ground,
            ratio_floor,
            margin,
            max_violated: 6,
            pending: None,
            log: Vec::new(),
        };
        let start = s.point(s.base.clone(), None)?;
        s.log.push(IterateLog {
            c_sigma: start.c_sigma,
            dc_sigma: start.dc_sigma.clone(),
            c_sigma_fresh: None,
            n_laplace_factorizations: start.n_laplace_factorizations,
        });
        s.current = start;
        Ok(s)
    }

    /// Build the bookkeeping at `mesh`. `fixed_fields` reuses the `X⁰`
    /// fields in [`RemorphMode::Fixed`].
    fn point(
        &self,
        mesh: TetMesh,
        fixed_fields: Option<&[Vec<[f64; 3]>]>,
    ) -> Result<Point, ElectrostaticError> {
        let (fields, n_fact) = match (self.mode, fixed_fields) {
            (RemorphMode::Fixed, Some(f)) => (f.to_vec(), 0),
            _ => {
                let ff = self.family.fields(&self.base, &mesh, self.mode)?;
                (ff.fields, ff.n_factorizations)
            }
        };
        let grad =
            capacitance_matrix_shape_gradient(&mesh, &self.eps_r, &self.conductors, &self.ground)?;
        let floating: Vec<usize> = (1..self.conductors.len()).collect();
        let red = grad.floating_reduction(0, &floating)?;
        let dc_sigma = fields.iter().map(|d| red.dc_dtheta(d)).collect();
        Ok(Point {
            mesh,
            fields,
            n_laplace_factorizations: n_fact,
            c_sigma: red.c_sigma,
            dc_sigma,
        })
    }

    fn eval_of(p: &Point) -> MultiEval {
        let de = d_e_c_hz_d_c_sigma(p.c_sigma);
        MultiEval {
            values: vec![e_c_hz_from_capacitance(p.c_sigma)],
            jacobian: vec![p.dc_sigma.iter().map(|g| de * g).collect()],
        }
    }

    fn trial_mesh(&self, dtheta: &[f64]) -> TetMesh {
        let mut m = self.current.mesh.clone();
        for (d, f) in dtheta.iter().zip(&self.current.fields) {
            if *d != 0.0 {
                m = apply_node_motion(&m, f, *d);
            }
        }
        m
    }

    /// The original mesh `X⁰`.
    pub fn base(&self) -> &TetMesh {
        &self.base
    }

    /// The current accepted mesh `X_k`.
    pub fn current_mesh(&self) -> &TetMesh {
        &self.current.mesh
    }

    /// The parameter fields built on the current iterate.
    pub fn current_fields(&self) -> &[Vec<[f64; 3]>] {
        &self.current.fields
    }

    /// The scalar-ε `C_Σ` (F) at the current iterate.
    pub fn c_sigma(&self) -> f64 {
        self.current.c_sigma
    }

    /// The re-morph mode.
    pub fn mode(&self) -> RemorphMode {
        self.mode
    }
}

impl MultiParamProblem for TransmonCSigmaProblem {
    fn n_params(&self) -> usize {
        self.family.n_params()
    }

    fn current(&self) -> MultiEval {
        Self::eval_of(&self.current)
    }

    fn check_step(&mut self, dtheta: &[f64]) -> StepCheck {
        let trial = self.trial_mesh(dtheta);
        let ratios = tet_volume_ratios(&self.base, &trial);
        let min_base = ratios.iter().copied().fold(f64::INFINITY, f64::min);
        let min_step = min_tet_volume_ratio(&self.current.mesh, &trial);
        if min_base >= self.ratio_floor {
            return StepCheck {
                feasible: true,
                min_quality_base: min_base,
                min_quality_step: min_step,
                violated: Vec::new(),
            };
        }
        let mut bad: Vec<(usize, f64)> = ratios
            .iter()
            .enumerate()
            .filter(|(_, r)| **r < self.ratio_floor)
            .map(|(t, r)| (t, *r))
            .collect();
        bad.sort_by(|a, b| a.1.total_cmp(&b.1));
        bad.truncate(self.max_violated);
        let tets: Vec<usize> = bad.iter().map(|(t, _)| *t).collect();
        let fields: Vec<&[[f64; 3]]> = self.current.fields.iter().map(Vec::as_slice).collect();
        let grads = tet_volume_ratio_derivatives(&self.base, &self.current.mesh, &tets, &fields);
        let now = tet_volume_ratios(&self.base, &self.current.mesh);
        let violated = tets
            .iter()
            .zip(grads)
            .map(|(&t, g)| StepConstraint {
                id: t,
                grad: g,
                rhs: self.ratio_floor + self.margin - now[t],
            })
            .collect();
        StepCheck {
            feasible: false,
            min_quality_base: min_base,
            min_quality_step: min_step,
            violated,
        }
    }

    fn evaluate_step(&mut self, dtheta: &[f64]) -> MultiEval {
        let trial = self.trial_mesh(dtheta);
        let fixed = (self.mode == RemorphMode::Fixed).then(|| self.current.fields.clone());
        let p = self
            .point(trial, fixed.as_deref())
            .expect("fresh gradient solve at the trial geometry");
        let e = Self::eval_of(&p);
        self.pending = Some((dtheta.to_vec(), p));
        e
    }

    fn accept_step(&mut self, dtheta: &[f64]) {
        let (d, p) = self
            .pending
            .take()
            .expect("accept_step without a pending evaluated step");
        assert_eq!(
            d, dtheta,
            "accept_step: step differs from the evaluated one"
        );
        self.log.push(IterateLog {
            c_sigma: p.c_sigma,
            dc_sigma: p.dc_sigma.clone(),
            c_sigma_fresh: None,
            n_laplace_factorizations: p.n_laplace_factorizations,
        });
        self.current = p;
    }

    fn confirm(&mut self) -> Option<Vec<f64>> {
        let c = fresh_c_sigma(
            &self.current.mesh,
            &self.eps_r,
            &self.conductors,
            &self.ground,
        )
        .ok()?;
        if let Some(l) = self.log.last_mut() {
            l.c_sigma_fresh = Some(c);
        }
        Some(vec![e_c_hz_from_capacitance(c)])
    }
}

/// An independent scalar-ε `C_Σ` on `mesh`: re-assemble, then
/// [`extract_capacitance`] (its own per-excitation solves), reduced over the
/// floating `conductors[1..]` (one floating conductor: `C_ii − C_if²/C_ff`).
///
/// # Errors
///
/// Propagates assembly / extraction errors, and
/// [`ElectrostaticError::ShapeMismatch`] for more than one floating conductor
/// (not needed here).
pub fn fresh_c_sigma(
    mesh: &TetMesh,
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
) -> Result<f64, ElectrostaticError> {
    let rho = vec![0.0; mesh.n_tets()];
    let sys = assemble_electrostatic(mesh, eps_r, &rho, conductors, ground)?;
    let cm = extract_capacitance(&sys, mesh, eps_r, conductors, ground, &[])?;
    let get = |a: &str, b: &str| {
        cm.get(a, b).ok_or_else(|| {
            ElectrostaticError::ShapeMismatch(format!("no capacitance entry ({a}, {b})"))
        })
    };
    let i = &conductors[0].name;
    match conductors.len() {
        1 => get(i, i),
        2 => {
            let f = &conductors[1].name;
            Ok(get(i, i)? - get(i, f)?.powi(2) / get(f, f)?)
        }
        n => Err(ElectrostaticError::ShapeMismatch(format!(
            "fresh_c_sigma: {n} conductors; only one floating conductor is supported"
        ))),
    }
}

/// [`fresh_c_sigma`] on a mesh and on its uniform red refinement
/// ([`red_refined_c_sigma`]).
#[derive(Clone, Copy, Debug)]
pub struct RefinedCSigma {
    /// `C_Σ` on the given mesh.
    pub coarse: f64,
    /// `C_Σ` on its red refinement (every tet split into 8, `h` halved).
    pub refined: f64,
    /// Fine-mesh tet count.
    pub n_tets: usize,
    /// Fine-mesh node count.
    pub n_nodes: usize,
    /// Fine tets oriented against their parent (0 for a valid refinement).
    pub n_flipped: usize,
}

impl RefinedCSigma {
    /// `(refined − coarse) / coarse`.
    pub fn rel_shift(&self) -> f64 {
        (self.refined - self.coarse) / self.coarse
    }
}

/// `C_Σ` before and after one uniform red refinement of `mesh`
/// ([`crate::mesh::red_refine`]), both through [`fresh_c_sigma`].
///
/// The conductors are sheets: `conductor_triangles[i]` (and
/// `ground_triangles`) are the surface triangles of `conductors[i]` (and of
/// the ground), and every edge midpoint of those triangles joins that
/// conductor on the fine mesh. `ε_r` is inherited from the parent tet. The
/// fine P1 space then contains the coarse one with the same Dirichlet and
/// floating constraints, so for this energy-minimum functional
/// `refined ≤ coarse` (to solver roundoff); the coarse value is an upper
/// bound on the converged one and the shift is a lower bound on the coarse
/// error. It says nothing about where the converged value lies.
///
/// # Errors
///
/// As [`fresh_c_sigma`], and [`ElectrostaticError::ShapeMismatch`] if
/// `conductor_triangles` does not have one entry per conductor.
///
/// # Panics
///
/// If a conductor triangle edge is not an edge of `mesh`.
pub fn red_refined_c_sigma(
    mesh: &TetMesh,
    eps_r: &[f64],
    conductors: &[Electrode],
    conductor_triangles: &[&[[u32; 3]]],
    ground: &[u32],
    ground_triangles: &[[u32; 3]],
) -> Result<RefinedCSigma, ElectrostaticError> {
    if conductor_triangles.len() != conductors.len() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "red_refined_c_sigma: {} triangle sets for {} conductors",
            conductor_triangles.len(),
            conductors.len()
        )));
    }
    let coarse = fresh_c_sigma(mesh, eps_r, conductors, ground)?;
    let r = red_refine(mesh);
    let fine_conductors: Vec<Electrode> = conductors
        .iter()
        .zip(conductor_triangles)
        .map(|(c, tris)| Electrode {
            name: c.name.clone(),
            nodes: r.lift_sheet_nodes(&c.nodes, tris),
            voltage: c.voltage,
        })
        .collect();
    let fine_ground = r.lift_sheet_nodes(ground, ground_triangles);
    let fine_eps = r.lift_per_tet(eps_r);
    let refined = fresh_c_sigma(&r.mesh, &fine_eps, &fine_conductors, &fine_ground)?;
    Ok(RefinedCSigma {
        coarse,
        refined,
        n_tets: r.mesh.n_tets(),
        n_nodes: r.mesh.n_nodes(),
        n_flipped: r.n_flipped,
    })
}

/// Physical island and cutout dimensions measured from a (possibly morphed)
/// fixture mesh, in mesh units.
#[derive(Clone, Copy, Debug)]
pub struct TransmonDimensions {
    /// Lowest island `y` (the junction lead's end; never moves).
    pub island_y_min: f64,
    /// Highest island `y` (the pad top).
    pub island_y_max: f64,
    /// `island_y_max − island_y_min`.
    pub island_length: f64,
    /// Pad width: `x` extent of the island nodes at `y ≥ y_pin + width_ramp`.
    pub pad_width: f64,
    /// Smallest cutout gap (edge `|x − axis|` minus the pad half-width) over
    /// the cutout-edge nodes where `theta_G`'s taper is 1.
    pub gap_min: f64,
    /// Largest such gap.
    pub gap_max: f64,
    /// The largest gap over the cutout-edge nodes where `theta_G`'s taper is 0
    /// (the ends of the moved run). Those edge nodes do not move, but the pad
    /// half-width it is measured from does, so this changes whenever
    /// `theta_W` does (30 µm at `X⁰`, 37.66 µm at #1036's headline design).
    pub gap_run_end: f64,
    /// Ground clearance above the pad top: lowest ground-sheet `y` above the
    /// pad (within the pad's `x` extent) minus `island_y_max`.
    pub top_gap: f64,
}

impl TransmonDimensions {
    /// Measure on `mesh` (same numbering as the roles' mesh).
    pub fn measure(mesh: &TetMesh, roles: &TransmonMorphRoles) -> Self {
        let s = &roles.spec;
        let p = |n: u32| mesh.nodes[n as usize];
        let (mut xlo, mut xhi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &n in &roles.island {
            xlo = xlo.min(p(n)[0]);
            xhi = xhi.max(p(n)[0]);
        }
        let axis = 0.5 * (xlo + xhi);
        let y_min = roles
            .island
            .iter()
            .map(|&n| p(n)[1])
            .fold(f64::INFINITY, f64::min);
        let y_max = roles
            .island
            .iter()
            .map(|&n| p(n)[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let (mut plo, mut phi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &n in &roles.island {
            if p(n)[1] >= s.y_pin + s.width_ramp - 1e-9 * s.y_pin.abs().max(1e-30) {
                plo = plo.min(p(n)[0]);
                phi = phi.max(p(n)[0]);
            }
        }
        let half = 0.5 * (phi - plo);
        let (y0, y1) = s.gap_run;
        let mut gmin = f64::INFINITY;
        let mut gmax = f64::NEG_INFINITY;
        let mut gend = f64::NEG_INFINITY;
        // Taper and run ends are judged on the node's y, which theta_G (pure
        // x motion) and the other fields (zero on ground) never change.
        for &n in &roles.gap_edge {
            let q = p(n);
            let g = (q[0] - axis).abs() - half;
            let tau = ((q[1] - y0) / s.gap_taper)
                .min((y1 - q[1]) / s.gap_taper)
                .clamp(0.0, 1.0);
            if tau >= 1.0 {
                gmin = gmin.min(g);
                gmax = gmax.max(g);
            } else if tau <= 0.0 {
                gend = gend.max(g);
            }
        }
        let top = roles
            .ground
            .iter()
            .map(|&n| p(n))
            .filter(|q| q[2].abs() <= 1e-12 && (q[0] - axis).abs() <= half && q[1] > y_max)
            .map(|q| q[1])
            .fold(f64::INFINITY, f64::min);
        Self {
            island_y_min: y_min,
            island_y_max: y_max,
            island_length: y_max - y_min,
            pad_width: phi - plo,
            gap_min: gmin,
            gap_max: gmax,
            gap_run_end: gend,
            top_gap: top - y_max,
        }
    }
}

/// The boundary of a triangle sheet as ordered node loops (edges used by one
/// triangle, chained), for plotting outlines. Loops are returned longest
/// first.
pub fn sheet_outline_loops(triangles: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let mut count: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut adj: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for ((a, b), c) in &count {
        if *c == 1 {
            adj.entry(*a).or_default().push(*b);
            adj.entry(*b).or_default().push(*a);
        }
    }
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut loops = Vec::new();
    for &start in adj.keys() {
        if seen.contains(&start) {
            continue;
        }
        let mut lp = vec![start];
        seen.insert(start);
        let mut prev = u32::MAX;
        let mut cur = start;
        loop {
            let next = adj[&cur]
                .iter()
                .copied()
                .find(|&n| n != prev && !seen.contains(&n));
            match next {
                Some(n) => {
                    lp.push(n);
                    seen.insert(n);
                    prev = cur;
                    cur = n;
                }
                None => break,
            }
        }
        loops.push(lp);
    }
    loops.sort_by_key(|l| std::cmp::Reverse(l.len()));
    loops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    fn pick(mesh: &TetMesh, f: impl Fn(&[f64; 3]) -> bool) -> Vec<u32> {
        mesh.nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| f(p))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// A four-parameter family on a cube. `a` and `b` prescribe the `x = 1`
    /// face and fix the `y = 1` face; `d` prescribes the `y = 1` face and
    /// fixes the `x = 1` face, so all three pin the same set (the union path:
    /// different prescribed subsets, one factorization). `c` prescribes the
    /// `y = 1` face but leaves the `x = 1` face free: a distinct set.
    fn cube_family(mesh: &TetMesh) -> MorphFamily {
        let tol = 1e-9;
        let face_x = pick(mesh, |p| (p[0] - 1.0).abs() < tol);
        let face_y: Vec<u32> = pick(mesh, |p| {
            (p[1] - 1.0).abs() < tol && p[0] > tol && p[0] < 1.0 - tol
        });
        let base_fixed = pick(mesh, |p| p[0].abs() < tol);
        let fx = face_x.clone();
        let fy = face_y.clone();
        let mut fixed_ab = base_fixed.clone();
        fixed_ab.extend(&face_y);
        let mut fixed_d = base_fixed.clone();
        fixed_d.extend(&face_x);
        MorphFamily::new(
            vec!["a".into(), "b".into(), "c".into(), "d".into()],
            vec![fixed_ab.clone(), fixed_ab, base_fixed, fixed_d],
            move |m, p| match p {
                0 => fx
                    .iter()
                    .map(|&n| (n, [0.0, 0.1 * m.nodes[n as usize][1], 0.0]))
                    .collect(),
                1 => fx
                    .iter()
                    .map(|&n| (n, [0.05 * m.nodes[n as usize][2], 0.0, 0.0]))
                    .collect(),
                2 => fy
                    .iter()
                    .map(|&n| (n, [0.0, 0.0, 0.07 * m.nodes[n as usize][0]]))
                    .collect(),
                _ => fy
                    .iter()
                    .map(|&n| (n, [0.03 * m.nodes[n as usize][2], 0.0, 0.0]))
                    .collect(),
            },
        )
    }

    #[test]
    fn family_groups_identical_pinned_sets_and_matches_single_solves() {
        let mesh = cube_tet_mesh(4, 1.0);
        let fam = cube_family(&mesh);
        let ff = fam.fields_of(&mesh, &[0, 1, 2, 3], None).unwrap();
        assert_eq!(ff.groups, vec![vec![0, 1, 3], vec![2]]);
        assert_eq!(ff.n_factorizations, 2, "one factorization per distinct set");
        for p in 0..4 {
            let single = super::super::harmonic_dirichlet_velocity(
                &mesh,
                &fam.data(&mesh, p),
                &fam.fixed_zero[p],
            )
            .unwrap();
            let scale = single
                .iter()
                .flat_map(|v| v.iter())
                .fold(0.0_f64, |m, x| m.max(x.abs()));
            let err = single
                .iter()
                .zip(&ff.fields[p])
                .flat_map(|(a, b)| (0..3).map(move |d| (a[d] - b[d]).abs()))
                .fold(0.0_f64, f64::max);
            assert!(scale > 0.0);
            assert!(
                err <= 1e-12 * scale,
                "param {p}: shared-factor field differs from the single solve by {err:e}"
            );
        }
    }

    #[test]
    fn remorph_budget_fixed_mode_is_the_straight_line() {
        let mesh = cube_tet_mesh(3, 1.0);
        let fam = cube_family(&mesh);
        let (b, _) =
            remorph_budget(&fam, 0, &mesh, RemorphMode::Fixed, -1.0, 0.25, 0.01, 50.0).unwrap();
        let f = fam.fields_of(&mesh, &[0], None).unwrap();
        let line = bisect_budget(&mesh, &f.fields[0], 0.25, -1.0, 50.0);
        assert_eq!(b.theta, line.theta);
        let (u, moved) =
            remorph_budget(&fam, 0, &mesh, RemorphMode::Unit, -1.0, 0.25, 0.05, 50.0).unwrap();
        assert!(u.n_steps > 1 && u.n_factorizations == u.n_steps);
        let r = min_tet_volume_ratio(&mesh, &moved);
        assert!(
            (r - 0.25).abs() < 1e-9 || u.unbounded,
            "the re-morph march must stop on the floor (ratio {r})"
        );
    }

    /// Every mesh face whose three nodes satisfy `on`.
    fn faces_on(mesh: &TetMesh, on: impl Fn(&[f64; 3]) -> bool) -> Vec<[u32; 3]> {
        let mut seen = BTreeSet::new();
        for t in &mesh.tets {
            for skip in 0..4 {
                let mut f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
                if f.iter().all(|&n| on(&mesh.nodes[n as usize])) {
                    f.sort_unstable();
                    seen.insert([f[0], f[1], f[2]]);
                }
            }
        }
        seen.into_iter().collect()
    }

    fn nodes_of(tris: &[[u32; 3]]) -> Vec<u32> {
        let mut v: Vec<u32> = tris.iter().flatten().copied().collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// A two-layer parallel plate (plates at `z = 0` and `z = 1`, side faces
    /// natural, interface at `z = 1/2` on a mesh plane): the exact potential
    /// is piecewise linear in `z`, so P1 is exact on both meshes and the red
    /// refinement must leave `C` unchanged to roundoff.
    #[test]
    fn red_refined_c_sigma_leaves_a_p1_exact_parallel_plate_unchanged() {
        let mesh = cube_tet_mesh(4, 1.0);
        let tol = 1e-12;
        let top = faces_on(&mesh, |p| (p[2] - 1.0).abs() < tol);
        let bottom = faces_on(&mesh, |p| p[2].abs() < tol);
        let eps: Vec<f64> = mesh
            .tets
            .iter()
            .map(|t| {
                let zc: f64 = t.iter().map(|&n| mesh.nodes[n as usize][2]).sum::<f64>() / 4.0;
                if zc < 0.5 { 1.0 } else { 3.0 }
            })
            .collect();
        let island = vec![Electrode {
            name: "island".into(),
            nodes: nodes_of(&top),
            voltage: 1.0,
        }];
        let r = red_refined_c_sigma(&mesh, &eps, &island, &[&top], &nodes_of(&bottom), &bottom)
            .unwrap();
        assert_eq!(r.n_tets, 8 * mesh.n_tets());
        assert_eq!(r.n_flipped, 0);
        let exact = crate::assembly::electrostatic::EPS_0 / (0.5 / 1.0 + 0.5 / 3.0);
        assert!(
            (r.coarse - exact).abs() <= 1e-10 * exact,
            "coarse {} vs exact {exact}",
            r.coarse
        );
        assert!(
            r.rel_shift().abs() <= 1e-12,
            "refinement moved a P1-exact C by {:e}",
            r.rel_shift()
        );
    }

    /// A sheet island inside a grounded box, with a second sheet below it
    /// absent, floating or grounded: the sheet edges are singular, so in every
    /// case refinement must lower `C_Σ` (nested P1 spaces, energy minimum) by
    /// a clearly resolved amount.
    #[test]
    fn red_refined_c_sigma_only_lowers_a_floating_sheet_c_sigma() {
        let mesh = cube_tet_mesh(8, 1.0);
        let tol = 1e-12;
        let in_sq = |p: &[f64; 3], z: f64| {
            (p[2] - z).abs() < tol && (0.25 - tol..=0.75 + tol).contains(&p[0]) && {
                (0.25 - tol..=0.75 + tol).contains(&p[1])
            }
        };
        let isl = faces_on(&mesh, |p| in_sq(p, 0.625));
        let fl = faces_on(&mesh, |p| in_sq(p, 0.375));
        let gnd = faces_on(&mesh, |p| {
            p.iter().any(|&x| x.abs() < tol || (x - 1.0).abs() < tol)
        });
        let eps = vec![1.0; mesh.n_tets()];
        let conductors = vec![
            Electrode {
                name: "island".into(),
                nodes: nodes_of(&isl),
                voltage: 1.0,
            },
            Electrode {
                name: "feedline".into(),
                nodes: nodes_of(&fl),
                voltage: 0.0,
            },
        ];
        let one = red_refined_c_sigma(
            &mesh,
            &eps,
            &conductors[..1],
            &[&isl],
            &nodes_of(&gnd),
            &gnd,
        )
        .unwrap();
        let two = red_refined_c_sigma(
            &mesh,
            &eps,
            &conductors,
            &[&isl, &fl],
            &nodes_of(&gnd),
            &gnd,
        )
        .unwrap();
        let mut gnd_fl = gnd.clone();
        gnd_fl.extend(&fl);
        let three = red_refined_c_sigma(
            &mesh,
            &eps,
            &conductors[..1],
            &[&isl],
            &nodes_of(&gnd_fl),
            &gnd_fl,
        )
        .unwrap();
        for (name, r) in [("no feedline", one), ("floating", two), ("grounded", three)] {
            assert_eq!(r.n_flipped, 0);
            assert!(
                r.rel_shift() < -1e-3,
                "{name}: refinement must lower C_Σ ({} -> {}, {:e})",
                r.coarse,
                r.refined,
                r.rel_shift()
            );
        }
        // A floating sheet raises C_Σ above "no sheet"; grounding it raises it
        // further. On both meshes.
        assert!(one.coarse < two.coarse && two.coarse < three.coarse);
        assert!(one.refined < two.refined && two.refined < three.refined);
        assert!(matches!(
            red_refined_c_sigma(&mesh, &eps, &conductors, &[&isl], &[], &[]),
            Err(ElectrostaticError::ShapeMismatch(_))
        ));
    }

    #[test]
    fn sheet_outline_of_a_square_is_one_loop() {
        // Two triangles of the unit square.
        let tris = [[0, 1, 2], [0, 2, 3]];
        let loops = sheet_outline_loops(&tris);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 4);
    }
}
