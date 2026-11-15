//! **Design A min-max driver** (Epic #1252 Phase 4c, issue #1578): the
//! piece that joins the two merged prerequisites of #1537 so an MMA run
//! can be posed at all.
//!
//! Pulled in with `#[path = "common/tcda_design.rs"] mod tcda_design;`, and
//! needs `common/periodic_fixtures.rs` (as `fixtures`) and
//! `common/tcda_baseline.rs` (as `tcda_baseline`) in scope of the
//! including test binary. #1579 (the design run) and #1580 (the CMA-ES
//! comparison) include it the same way.
//!
//! # What it joins
//!
//! * The **baseline** of #1536 is a lumped delta-gap element in the repo's
//!   normalized units (`λ₀ = 2` at `ω₀ = π`, pitch 0.9), and its recorded
//!   worst case (−3.50 dB) and the Phase-4 kill criterion are defined on
//!   `Γ = (Z − z_ref)/(Z + z_ref)` with `Z = edge_scan_impedance`.
//! * The **parameterization** of #1535 (`FloquetElementParam`) is in
//!   millimetres: its `MIN_TRACE_MM = MIN_GAP_MM = 0.15` compare against
//!   the same lengths the outline columns are built from.
//! * The **adjoint** of #1578 (`floquet_edge_gamma_vjp`) differentiates
//!   exactly the baseline's figure of merit.
//!
//! The join is by **scale, not by changing either**: the baseline mesh is
//! built with every length multiplied by [`MM_PER_UNIT`] and the solve runs
//! at `ω/MM_PER_UNIT` (`tcda_baseline::build_cell_scaled`). The scan
//! impedance is exactly scale-invariant in this unit system, so the
//! recorded numbers reproduce (asserted by
//! `tcda_design::baseline_reproduces_at_zero_displacement`). Without the
//! scale the limits would be nonsense: in normalized units the baseline's
//! coupling gap is `0.9 − 2·0.39 = 0.12`, already below a "0.15" read as a
//! normalized length.
//!
//! [`MM_PER_UNIT`] is pinned by **one** choice: the frequency at which the
//! `STANDARD_STACK` permittivities are quoted, 10 GHz. `λ₀ = 2` normalized
//! units must then be `c/10 GHz = 29.9792458 mm`, so one normalized unit is
//! 14.9896229 mm and the element is a 13.49 mm cell — a plausible X-band
//! TCDA pitch (0.45 λ₀). Any documented value works; this one makes the
//! laminate table apply at face value.
//!
//! # The driver
//!
//! [`TcdaDesign`] is the `MinMaxPoints` implementation: `values` is one
//! assembly plus one factorization per grid point, `gradient` is one
//! `floquet_edge_gamma_vjp` (one factorization, one transpose back-solve)
//! for the point asked for. The design vector is `FloquetElementParam`'s:
//! `θ = [ε′ per layer | right | left | top | bottom]`, and the adjoint
//! returns its gradient in exactly that order (the material block comes
//! first in both). [`ParamSpec`] groups the parameterization knobs;
//! `ParamSpec::default()` is the no-material-layer shape the reproduction
//! and smoke runs use.
//!
//! [`ConstrainedEpigraph`] stacks the #1535 trace / gap / stack rows under
//! the epigraph's worst-case rows, so MMA sees the manufacturability
//! constraints and the min-max constraints as one problem.
//!
//! # Honesty clause
//!
//! `values` returns an `EvalError`, not a number, when the morphed mesh has
//! an inverted tet (`min_tet_volume_ratio ≤ 0`): a solve on an inverted
//! mesh is meaningless, and MMA backs the step off instead of optimizing a
//! fiction (the #589 rule).
#![allow(dead_code)] // each test binary uses a subset

use faer::c64;

use geode_core::driven::floquet::FloquetCell;
use geode_core::driven::floquet_sensitivity::{
    FloquetDesign, FloquetGeometry, FloquetSensitivityOptions, floquet_edge_gamma_vjp,
};
use geode_core::mesh::TetMesh;
use geode_core::shape::floquet_element::{
    ElementGeometry, FloquetElementParam, StackLayer, StandardMaterial,
};
use geode_core::shape::min_tet_volume_ratio;
use geode_optimize::{Epigraph, EvalError, Evaluation, MinMaxPoints, Objective};

use super::tcda_baseline::{BaselineParams, BaselineParts, ScanGrid, build_parts};

/// Speed of light in mm/ns, i.e. mm · GHz.
const C_MM_GHZ: f64 = 299.792_458;
/// The frequency the `STANDARD_STACK` permittivities are quoted at (GHz).
pub const STACK_FREQ_GHZ: f64 = 10.0;
/// Millimetres per normalized unit: `λ₀ = 2` units is `c/f` at
/// [`STACK_FREQ_GHZ`] (module docs). About 14.99 mm, a 13.49 mm cell.
pub const MM_PER_UNIT: f64 = C_MM_GHZ / STACK_FREQ_GHZ / 2.0;

/// One `(ω, θ, φ)` point of the driver's grid. `omega` is **normalized**
/// (`ω₀ = π`); the driver converts it with the build scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanPoint {
    pub omega: f64,
    pub theta: f64,
    pub phi: f64,
}

/// The points of `grid` in the order `tcda_baseline::max_gamma_active`
/// walks them (`ω` outer, `φ` fastest), so a driver index and a `WorstCase`
/// index name the same point.
pub fn grid_points(grid: &ScanGrid) -> Vec<ScanPoint> {
    let mut out = Vec::with_capacity(grid.n_points());
    for &omega in &grid.omegas {
        for &theta in &grid.thetas {
            for &phi in &grid.phis {
                out.push(ScanPoint { omega, theta, phi });
            }
        }
    }
    out
}

/// The knobs of the #1535 parameterization the driver builds, grouped so
/// [`TcdaDesign::new`] stays a short call.
///
/// [`Self::default`] is the shape the reproduction and smoke runs use: no
/// material block (the baseline superstrate stays at its `ε′ = 2.0`, which
/// is not a standard laminate), one spline coefficient per outline edge, so
/// four design variables.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// Superstrate material block. `vec![]` keeps the baseline stack fixed;
    /// one layer tunes the whole slab as a single `ε′` region. More than one
    /// is rejected (the baseline superstrate is one slab).
    pub layers: Vec<StackLayer>,
    /// Spline coefficients per outline edge (`4·n_edge` outline variables).
    pub n_edge: usize,
    /// Constraint samples per edge (`≥ 2`).
    pub n_samples: usize,
    /// KS sharpness of the folded trace / gap constraints (1/mm).
    pub beta: f64,
}

impl Default for ParamSpec {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            n_edge: 1,
            n_samples: 4,
            beta: 20.0,
        }
    }
}

/// The min-max driver: `a_k(θ) = |Γ_active(ω_k, θ_k, φ_k)|²` of the scaled
/// baseline element under `FloquetElementParam`, with the lumped-feed
/// adjoint for the gradient. See the module docs.
pub struct TcdaDesign {
    parts: BaselineParts,
    param: FloquetElementParam,
    /// `∂X/∂θ_outline` on the **base** mesh: the exact Jacobian of the
    /// linear map `X(θ) = X₀ + Vθ`, so it is built once and never remorphed.
    design: FloquetDesign,
    /// Number of `ε′` layers (0 or 1); the leading block of `θ`.
    n_layers: usize,
    z_ref: f64,
    grid: Vec<ScanPoint>,
    /// Factorizations the gradient pass has performed.
    pub n_factorizations: usize,
    /// Cells the driver has assembled (forward + adjoint passes).
    pub n_assemblies: usize,
}

impl TcdaDesign {
    /// Build the driver over the scaled baseline cell of `p`, with the
    /// parameterization of `spec` and the scan grid `grid`.
    ///
    /// # Errors
    ///
    /// A string message if `spec` asks for more than one layer (the
    /// baseline superstrate is a single slab; banding it is #1579's
    /// business), if `z_ref` is not positive, if the grid is empty, or if
    /// `FloquetElementParam` / the adjoint design rejects the element.
    pub fn new(
        p: &BaselineParams,
        scale: f64,
        spec: ParamSpec,
        z_ref: f64,
        grid: &ScanGrid,
    ) -> Result<Self, String> {
        let ParamSpec {
            layers,
            n_edge,
            n_samples,
            beta,
        } = spec;
        if layers.len() > 1 {
            return Err(format!(
                "the baseline superstrate is one slab, so this driver takes 0 or 1 layer (got {})",
                layers.len()
            ));
        }
        if !(z_ref > 0.0 && z_ref.is_finite()) {
            return Err(format!("z_ref must be finite and positive (got {z_ref})"));
        }
        let points = grid_points(grid);
        if points.is_empty() {
            return Err("the scan grid is empty".into());
        }
        let parts = build_parts(p, scale);
        let g = parts.geom;
        let n_layers = layers.len();
        let param = FloquetElementParam::new(
            ElementGeometry {
                period: [g.pitch, g.pitch],
                z_range: [0.0, g.h_top],
                z_copper: g.h_dip,
                patch_x: g.patch_x,
                patch_y: g.patch_y,
            },
            layers,
            n_edge,
            n_samples,
            beta,
        )
        .map_err(|e| e.to_string())?;
        // One ε′ region covering the whole superstrate, or none.
        let layer_of_tet: Vec<Option<usize>> = parts
            .superstrate
            .iter()
            .map(|&inside| (inside && n_layers > 0).then_some(0))
            .collect();
        let design = param
            .design(&parts.mesh, layer_of_tet)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            parts,
            param,
            design,
            n_layers,
            z_ref,
            grid: points,
            n_factorizations: 0,
            n_assemblies: 0,
        })
    }

    /// The parameterization (for the constraints and `n_params`).
    pub fn param(&self) -> &FloquetElementParam {
        &self.param
    }

    /// The baseline parts the driver morphs.
    pub fn parts(&self) -> &BaselineParts {
        &self.parts
    }

    /// The grid, in driver index order.
    pub fn points(&self) -> &[ScanPoint] {
        &self.grid
    }

    /// The feed-line reference impedance.
    pub fn z_ref(&self) -> f64 {
        self.z_ref
    }

    /// `θ₀`: the baseline element itself — zero outline displacement, and
    /// (if a layer is declared) its `ε′` at the midpoint of the laminate's
    /// range, as `FloquetElementParam::initial_theta`.
    pub fn initial_theta(&self) -> Vec<f64> {
        self.param.initial_theta()
    }

    /// The morphed node coordinates and per-tet `ε` at `theta`.
    ///
    /// # Errors
    ///
    /// [`EvalError`] on a wrong `θ` length or an inverted tet (module docs:
    /// the honesty clause).
    pub fn morph(&self, theta: &[f64]) -> Result<(Vec<[f64; 3]>, Vec<c64>), EvalError> {
        let nodes = self.morphed_nodes(theta)?;
        // The guard of #589: an inverted tet is an evaluation error, never a
        // number (`min_tet_volume_ratio` keeps the base orientation).
        let mut moved = self.parts.mesh.clone();
        moved.nodes.copy_from_slice(&nodes);
        let ratio = min_tet_volume_ratio(&self.parts.mesh, &moved);
        if ratio <= 0.0 || !ratio.is_finite() {
            return Err(EvalError::new(format!(
                "the morphed mesh has an inverted or degenerate tet: min signed-volume ratio \
                 {ratio:.3e} ≤ 0"
            )));
        }
        let eps: Vec<c64> = if self.n_layers == 0 {
            self.parts.eps.clone()
        } else {
            self.parts
                .superstrate
                .iter()
                .map(|&inside| c64::new(if inside { theta[0] } else { 1.0 }, 0.0))
                .collect()
        };
        Ok((nodes, eps))
    }

    /// The mesh morphed to `theta`: length and finiteness checked, the
    /// outline columns applied — **no** inversion check. [`Self::morph`]
    /// adds the #589 honesty clause on top; [`Self::min_vol_ratio`] reads
    /// the raw ratio a caller needs to report an infeasible design.
    fn morphed_nodes(&self, theta: &[f64]) -> Result<Vec<[f64; 3]>, EvalError> {
        if theta.len() != self.param.n_params() {
            return Err(EvalError::new(format!(
                "θ has {} entries, the parameterization has {}",
                theta.len(),
                self.param.n_params()
            )));
        }
        if let Some(i) = theta.iter().position(|v| !v.is_finite()) {
            return Err(EvalError::new(format!("θ[{i}] is not finite")));
        }
        let shape = self
            .design
            .shape
            .as_ref()
            .ok_or_else(|| EvalError::new("the design has no outline columns"))?;
        let off = self.param.outline_offset();
        let mut nodes = self.parts.mesh.nodes.clone();
        for c in 0..shape.n_columns() {
            let t = theta[off + c];
            if t == 0.0 {
                continue;
            }
            for (node, v) in shape.column(c).iter().enumerate() {
                for k in 0..3 {
                    nodes[node][k] += t * v[k];
                }
            }
        }
        Ok(nodes)
    }

    /// The minimum signed-tet-volume ratio of the mesh morphed to `theta`,
    /// **without** the [`EvalError`] [`Self::morph`] raises on a non-positive
    /// one: `≤ 0` means an inverted or degenerate tet, and a caller that
    /// answers *infeasible* designs over a protocol (the #1580 eval server)
    /// needs the quantity, not a panic. Length and finiteness of `theta`
    /// remain errors.
    pub fn min_vol_ratio(&self, theta: &[f64]) -> Result<f64, EvalError> {
        let nodes = self.morphed_nodes(theta)?;
        let mut moved = self.parts.mesh.clone();
        moved.nodes.copy_from_slice(&nodes);
        Ok(min_tet_volume_ratio(&self.parts.mesh, &moved))
    }

    /// The cell at `theta` together with the mesh it was assembled on (the
    /// adjoint needs both).
    ///
    /// # Errors
    ///
    /// As [`Self::morph`], plus a stringified assembly failure.
    pub fn assemble(
        &mut self,
        theta: &[f64],
    ) -> Result<(FloquetCell, TetMesh, Vec<c64>), EvalError> {
        let (nodes, eps) = self.morph(theta)?;
        let cell = self.parts.assemble(&nodes, &eps).map_err(EvalError::new)?;
        self.n_assemblies += 1;
        let mut mesh = self.parts.mesh.clone();
        mesh.nodes.copy_from_slice(&nodes);
        Ok((cell, mesh, eps))
    }

    /// `Γ_active` of `cell` at one grid point, at the mesh's own `ω`.
    ///
    /// # Errors
    ///
    /// The Floquet solve error, stringified.
    pub fn gamma_at(&self, cell: &FloquetCell, pt: ScanPoint) -> Result<c64, EvalError> {
        let z = cell
            .edge_scan_impedance(
                self.parts.omega_of(pt.omega),
                pt.theta,
                pt.phi,
                self.parts.feed_edge,
            )
            .map_err(|e| EvalError::new(e.to_string()))?;
        let zr = c64::new(self.z_ref, 0.0);
        Ok((z - zr) / (z + zr))
    }

    /// `FloquetEdgeVjp` at one grid point of the design `theta`: the raw
    /// adjoint result, for the FD check.
    ///
    /// # Errors
    ///
    /// As [`Self::assemble`], plus the adjoint's own errors (stringified).
    pub fn vjp_at(
        &mut self,
        k: usize,
        theta: &[f64],
    ) -> Result<geode_core::driven::floquet_sensitivity::FloquetEdgeVjp, EvalError> {
        let pt = *self
            .grid
            .get(k)
            .ok_or_else(|| EvalError::new(format!("grid point {k} is out of range")))?;
        let (cell, mesh, eps) = self.assemble(theta)?;
        let geo = FloquetGeometry {
            mesh: &mesh,
            map: &self.parts.map,
            eps_r: &eps,
            sigma: None,
        };
        let vjp = floquet_edge_gamma_vjp(
            &cell,
            &geo,
            self.parts.omega_of(pt.omega),
            pt.theta,
            pt.phi,
            self.parts.feed_edge,
            self.z_ref,
            &self.design,
            &FloquetSensitivityOptions::default(),
        )
        .map_err(|e| EvalError::new(e.to_string()))?;
        self.n_factorizations += vjp.n_factorizations;
        Ok(vjp)
    }
}

impl MinMaxPoints for TcdaDesign {
    fn dim(&self) -> usize {
        self.param.n_params()
    }

    fn n_points(&self) -> usize {
        self.grid.len()
    }

    fn values(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let (cell, _, _) = self.assemble(x)?;
        let mut out = Vec::with_capacity(self.grid.len());
        for i in 0..self.grid.len() {
            let pt = self.grid[i];
            out.push(self.gamma_at(&cell, pt)?.norm_sqr());
        }
        Ok(out)
    }

    fn gradient(&mut self, k: usize, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let vjp = self.vjp_at(k, x)?;
        if vjp.grad.len() != self.param.n_params() {
            return Err(EvalError::new(format!(
                "the adjoint returned {} columns, θ has {}",
                vjp.grad.len(),
                self.param.n_params()
            )));
        }
        Ok(vjp.grad)
    }
}

/// [`Epigraph`] over a [`TcdaDesign`] with the #1535 manufacturability
/// constraints stacked underneath: the `K` worst-case rows `a_k − t ≤ 0`
/// first, then the KS-folded trace / gap rows and the per-layer `ε′` bounds
/// (flat in the epigraph variable `t`).
pub struct ConstrainedEpigraph {
    inner: Epigraph<TcdaDesign>,
    param: FloquetElementParam,
    n_element: usize,
}

impl ConstrainedEpigraph {
    /// Wrap `points`; `n_element` is read off the parameterization.
    ///
    /// # Errors
    ///
    /// A stringified `FloquetElementParam::constraints` failure at `θ₀`.
    pub fn new(points: TcdaDesign) -> Result<Self, String> {
        Self::build(points, None)
    }

    /// As [`Self::new`], but the epigraph computes a gradient only for the
    /// points with `a_k ≥ max(t, max_j a_j) − margin`
    /// ([`Epigraph::with_active_margin`]). The manufacturability rows are
    /// unaffected: they are analytic and cost no solve.
    ///
    /// This is the cost knob #1579's run record names. It is a **trade**,
    /// not a free saving — see the `Epigraph` module docs: a skipped point
    /// is linearized as constant in `θ` for that iteration, so a step can
    /// raise it above the band, and the optimizer only sees that at the
    /// next evaluation. The default ([`Self::new`]) is exact.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn with_active_margin(points: TcdaDesign, margin: f64) -> Result<Self, String> {
        Self::build(points, Some(margin))
    }

    fn build(points: TcdaDesign, margin: Option<f64>) -> Result<Self, String> {
        let param = points.param().clone();
        let n_element = param
            .constraints(&param.initial_theta())
            .map_err(|e| e.to_string())?
            .g
            .len();
        let inner = Epigraph::new(points);
        let inner = match margin {
            Some(m) => inner.with_active_margin(m),
            None => inner,
        };
        Ok(Self {
            inner,
            param,
            n_element,
        })
    }

    /// The starting design `[θ, max_k a_k(θ)]` (see `Epigraph`).
    ///
    /// # Errors
    ///
    /// The forward pass failed at `theta`.
    pub fn initial_design(&mut self, theta: &[f64]) -> Result<Vec<f64>, EvalError> {
        self.inner.initial_design(theta)
    }

    /// Per-point gradients (adjoint solves) computed so far.
    pub fn n_gradients(&self) -> usize {
        self.inner.n_gradients()
    }

    /// The wrapped driver.
    pub fn points(&self) -> &TcdaDesign {
        self.inner.points()
    }

    /// The number of manufacturability rows under the worst-case rows.
    pub fn n_element_constraints(&self) -> usize {
        self.n_element
    }
}

impl Objective for ConstrainedEpigraph {
    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn n_constraints(&self) -> usize {
        self.inner.n_constraints() + self.n_element
    }

    fn evaluate(&mut self, d: &[f64]) -> Result<Evaluation, EvalError> {
        let n = self.inner.dim() - 1;
        if d.len() != n + 1 {
            return Err(EvalError::new(format!(
                "design has {} entries, expected {}",
                d.len(),
                n + 1
            )));
        }
        let mut ev = self.inner.evaluate(d)?;
        let ec = self
            .param
            .constraints(&d[..n])
            .map_err(|e| EvalError::new(e.to_string()))?;
        for (g, mut row) in ec.g.into_iter().zip(ec.jac) {
            row.push(0.0); // the manufacturability rows do not involve t
            ev.constraints.push(g);
            ev.jacobian.push(row);
        }
        Ok(ev)
    }
}

/// The `STANDARD_STACK` entry of `material`, for a test that wants to state
/// a layer's allowed `ε′` range.
pub fn standard(material: usize) -> &'static StandardMaterial {
    &geode_core::shape::floquet_element::STANDARD_STACK[material]
}

// ---------------------------------------------------------- the eval server

/// **Design A eval server** (issue #1580): the JSON-lines protocol
/// `patch_conformal_nested_dof --eval-server` introduced (#1130), answered
/// from a [`TcdaDesign`] so
/// `benchmarks/patch_antenna_conformal/scripts/cma_loop.py` can drive this
/// problem unmodified. Each stdin line is `f x1 … xn` (forward: feasibility,
/// objective, worst case) or `v x1 … xn` (the whole grid with per-point
/// complex `Γ_active`, the verification sweep); each answer is one JSON line
/// on `out`. `q` quits.
///
/// Feasibility is the Design A rule, decided here before any solve: no
/// inverted tet ([`TcdaDesign::min_vol_ratio`] `> 0`) and the sampled #1535
/// manufacturability rows not violated. An infeasible design is answered
/// without a solve, exactly like the paper-1 volume guard. `solves` is the
/// cumulative count of factorizations on the `cma_loop.py` rule: one per
/// grid point of every forward or verification sweep (a mid-sweep solver
/// error is answered `feasible: false` with its reason and counts nothing —
/// the exact partial count is not observable from outside the driver).
///
/// The objective of a feasible design is `max_k |Γ_active(k)|²` — the true
/// min-max value, the same quantity the #1579 epigraph's worst-case rows
/// minimize — so a CMA-ES run on it is directly comparable with the adjoint
/// run at equal solves. A `v` sweep is answered for any *meshable* design
/// (manufacturability aside), the paper-1 semantics; an inverted mesh has no
/// sweep to answer with.
///
/// # Errors
///
/// Propagates [`std::io::Error`] from `input`/`out`; malformed protocol
/// lines and designs panic, like the paper-1 server.
pub fn run_eval_server(
    drv: &mut TcdaDesign,
    input: &mut dyn std::io::BufRead,
    out: &mut dyn std::io::Write,
) -> std::io::Result<()> {
    use std::f64::consts::PI;
    use std::fmt::Write as _;
    use std::time::Instant;

    use geode_core::testing::{TestBackend, test_device};
    use geode_util::fixture::BackendInfo;

    let n = drv.dim();
    let pts: Vec<ScanPoint> = drv.points().to_vec();
    let k = pts.len();
    let backend = BackendInfo::of::<TestBackend>(&test_device());
    let list = |vs: &[f64]| {
        vs.iter()
            .map(|v| format!("{v:.9}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        out,
        "{{\"ready\": true, \"n_dofs\": {n}, \"grid\": {{\"omega_over_pi\": [{}], \
         \"theta_deg\": [{}], \"phi_deg\": [{}]}}, \"external\": false, \
         \"backend\": \"{}\", \"float_dtype\": \"{}\", \"generated_at_commit\": \"{}\"}}",
        list(&pts.iter().map(|p| p.omega / PI).collect::<Vec<_>>()),
        list(&pts.iter().map(|p| p.theta.to_degrees()).collect::<Vec<_>>()),
        list(&pts.iter().map(|p| p.phi.to_degrees()).collect::<Vec<_>>()),
        backend.name,
        backend.float_dtype,
        geode_util::repo::current_commit(),
    );
    out.flush()?;
    let num = |v: f64| {
        if v.is_finite() {
            format!("{v:.12e}")
        } else {
            "null".to_string()
        }
    };
    let mut solves = 0usize;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        let mut it = line.split_whitespace();
        let Some(kind) = it.next() else { continue };
        if kind == "q" {
            break;
        }
        assert!(
            matches!(kind, "f" | "v"),
            "tcda eval-server: line must start with f, v or q"
        );
        let x: Vec<f64> = it.map(|v| v.parse().expect("design value")).collect();
        assert_eq!(x.len(), n, "eval-server: expected {n} design values");
        let t0 = Instant::now();
        // Feasibility first — neither check costs a solve. The ratio is
        // clamped finite for the JSON (no NaN in JSON); a non-finite ratio
        // is an inverted mesh a fortiori and -1.0 ranks it last.
        let vr_raw = drv.min_vol_ratio(&x).expect("a checked design vector");
        let vr = if vr_raw.is_finite() { vr_raw } else { -1.0 };
        let meshable = vr > 0.0;
        let mut t = format!("{{\"min_vol_ratio\": {}", num(vr));
        if !meshable {
            let _ = write!(t, ", \"feasible\": false, \"reason\": \"inverted tet\"");
        } else if kind == "v" {
            // Verification sweep: the whole grid, per-point complex Γ,
            // manufacturability aside (the paper-1 semantics).
            let (cell, _, _) = drv
                .assemble(&x)
                .map_err(|e| std::io::Error::other(e.message))?;
            let mut point = Vec::with_capacity(k);
            let mut g = Vec::with_capacity(k);
            let mut gamma = Vec::with_capacity(k);
            for pt in &pts {
                let gam = drv.gamma_at(&cell, *pt).expect("a meshable sweep");
                point.push(format!(
                    "[{:.9}, {:.9}, {:.9}]",
                    pt.omega / PI,
                    pt.theta.to_degrees(),
                    pt.phi.to_degrees()
                ));
                g.push(num(gam.norm_sqr()));
                gamma.push(format!("[{}, {}]", num(gam.re), num(gam.im)));
            }
            solves += k;
            let _ = write!(
                t,
                ", \"point\": [{}], \"g\": [{}], \"gamma\": [{}]",
                point.join(", "),
                g.join(", "),
                gamma.join(", ")
            );
        } else {
            let cons = drv
                .param()
                .constraints_sampled(&x)
                .expect("a checked design vector");
            if cons.is_violated() {
                let _ = write!(
                    t,
                    ", \"feasible\": false, \"reason\": \"manufacturability\", \
                     \"constraint_violation_mm\": {}",
                    num(cons.max_violation())
                );
            } else {
                let a = drv
                    .values(&x)
                    .map_err(|e| std::io::Error::other(e.message))?;
                assert_eq!(a.len(), k);
                let worst = a.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v));
                solves += k;
                let _ = write!(
                    t,
                    ", \"feasible\": true, \"objective\": {}, \"worst_db\": {}, \"a\": [{}]",
                    num(worst),
                    num(10.0 * worst.max(f64::MIN_POSITIVE).log10()),
                    a.iter().map(|&v| num(v)).collect::<Vec<_>>().join(", ")
                );
            }
        }
        let _ = writeln!(
            t,
            ", \"solves\": {}, \"wall_s\": {:.3}}}",
            solves,
            t0.elapsed().as_secs_f64()
        );
        out.write_all(t.as_bytes())?;
        out.flush()?;
    }
    Ok(())
}
