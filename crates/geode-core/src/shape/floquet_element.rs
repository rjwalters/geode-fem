//! **Floquet element parameterization and PCB manufacturability constraints**
//! (Epic #1252 Phase 4, issue #1535).
//!
//! Maps a design vector `θ` to node motion of the copper outline of one flat
//! Floquet unit cell plus the permittivity of its superstrate layers, in the
//! form the P1 adjoint consumes
//! ([`FloquetDesign`](crate::driven::floquet_sensitivity::FloquetDesign)),
//! and exposes differentiable min-trace / min-gap / stack constraints
//! `g(θ) ≤ 0` with their Jacobian for MMA
//! (`geode_optimize::Evaluation::constrained`).
//!
//! # Parameterization
//!
//! The copper is one rectangular patch outline `[xl, xr] × [yl, yh]` in the
//! cell `[0, Px] × [0, Py]`, at height `z_copper`. Each of its four edges is
//! displaced **outward** along its normal by a clamped B-spline
//! ([`BSpline1D`]) of `n_edge` coefficients along the edge. The design
//! vector is
//!
//! ```text
//! θ = [ ε′_layer0 … ε′_layer(L−1) | right (n_edge) | left | top | bottom ]
//! ```
//!
//! The material block comes first because that is the column order of
//! [`floquet_s_vjp`](crate::driven::floquet_sensitivity::floquet_s_vjp)
//! (every region's `ε′`, then the shape columns), so `grad` of the adjoint is
//! directly `dg/dθ`. The `ε′` entries are absolute values; the outline
//! entries are displacements (mm) of the base mesh, so the node map
//! `X(θ) = X₀ + V θ_outline` is linear and the columns `V` are its exact
//! Jacobian.
//!
//! Node motion is the product of three smooth tapers (all [`smoothstep`]):
//! along the edge normal it is 0 at the patch centre, 1 on the edge and decays
//! to 0 at the cell wall; along the edge it is 1 over the patch extent and
//! decays to 0 at the cell wall; along `z` it is 1 on the copper plane and 0
//! at both Floquet ports. Hence **port faces and periodic faces are pinned
//! exactly** (a requirement of `driven/floquet_sensitivity.rs`) and nodes on
//! the outline move by exactly the spline value.
//!
//! The Helmholtz filter and the cylinder-specific types of
//! [`super::conformal_basis`] are not used: they are tied to the bent
//! `PatchFixture`. Only [`BSpline1D`] and [`smoothstep`] are reused.
//!
//! # Manufacturability constraints
//!
//! With one patch per cell the outline widths are linear in `θ`:
//! `w_x(y) = (xr−xl) + B(y)·(θ_R + θ_L)` and likewise `w_y(x)`. The copper
//! **trace** is the smaller width, the **gap** to the periodic neighbour is
//! `P − w` at the same `y` (`x`). Both are sampled at `n_samples` points per
//! edge. Constraints, all `≤ 0` when feasible and in mm:
//!
//! * trace: `MIN_TRACE_MM − w_k`, gap: `MIN_GAP_MM − (P − w_k)`;
//! * stack: `ε_min − ε′`, `ε′ − ε_max` per layer, with the range of the
//!   layer's standard material ([`STANDARD_STACK`]).
//!
//! [`FloquetElementParam::constraints_sampled`] returns every sample (exact:
//! `g = 0` at exactly the limit). [`FloquetElementParam::constraints`] folds
//! the trace samples and the gap samples with a Kreisselmeier–Steinhauser
//! soft-max, `g = ln(Σ exp(β g_k))/β`, giving 2 + 2·L smooth constraints.
//! `g_KS ≥ max g_k` (never under-reports a violation) and
//! `g_KS − max g_k ≤ ln(N)/β`, the stated tolerance
//! ([`FloquetElementParam::ks_tolerance`]).
//!
//! # Standard stack table
//!
//! Real part of the relative permittivity at 10 GHz and the stock core
//! thicknesses, from the manufacturers' public datasheets (Isola FR-4 class
//! laminates, Rogers RO4003C / RO4350B / RO3003 data sheets). The ε range is
//! the datasheet design value ± its tolerance band, so a design may tune `ε′`
//! only inside what the process actually delivers. This is a design-time
//! table, not a procurement guarantee.
//!
//! # Scope
//!
//! Single-patch rectilinear outline (trace = patch width, gap = periodic
//! clearance). Corner features and multi-island layouts are not modelled.
//! The thickness of a layer is a discrete table choice baked into the mesh,
//! not a continuous parameter.

use crate::driven::floquet_sensitivity::FloquetDesign;
use crate::driven::s_sensitivity::{MaterialDesign, SSensitivityError, ShapeDesign};
use crate::mesh::TetMesh;

use super::conformal_basis::{BSpline1D, smoothstep};

/// Minimum copper trace width (mm).
pub const MIN_TRACE_MM: f64 = 0.15;
/// Minimum copper gap (mm).
pub const MIN_GAP_MM: f64 = 0.15;

/// A standard laminate: name, `ε′` design range `[min, max]` and the stock
/// core thicknesses (mm). See the module docs for the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardMaterial {
    /// Laminate name.
    pub name: &'static str,
    /// Lowest allowed `ε′`.
    pub eps_min: f64,
    /// Highest allowed `ε′`.
    pub eps_max: f64,
    /// Stock thicknesses (mm).
    pub thicknesses_mm: &'static [f64],
}

/// The allowed superstrate / substrate laminates.
pub const STANDARD_STACK: [StandardMaterial; 4] = [
    StandardMaterial {
        name: "FR-4",
        eps_min: 4.1,
        eps_max: 4.7,
        thicknesses_mm: &[0.2, 0.4, 0.8, 1.6],
    },
    StandardMaterial {
        name: "RO4003C",
        eps_min: 3.33,
        eps_max: 3.43,
        thicknesses_mm: &[0.203, 0.305, 0.406, 0.508, 0.813, 1.524],
    },
    StandardMaterial {
        name: "RO4350B",
        eps_min: 3.43,
        eps_max: 3.53,
        thicknesses_mm: &[0.168, 0.254, 0.338, 0.508, 0.762, 1.524],
    },
    StandardMaterial {
        name: "RO3003",
        eps_min: 2.97,
        eps_max: 3.03,
        thicknesses_mm: &[0.127, 0.25, 0.5, 0.75, 1.52],
    },
];

/// One superstrate layer: a standard material and a stock thickness.
#[derive(Debug, Clone)]
pub struct StackLayer {
    /// Index into [`STANDARD_STACK`].
    pub material: usize,
    /// Thickness (mm); must be a stock thickness of the material.
    pub thickness_mm: f64,
}

/// Errors of the element parameterization.
#[derive(Debug, thiserror::Error)]
pub enum FloquetElementError {
    /// Inconsistent geometry or parameter-count input.
    #[error("invalid Floquet element: {0}")]
    Invalid(String),
    /// A layer is not a standard FR-4 / Rogers layer.
    #[error("layer {layer}: {why}")]
    NonStandardLayer {
        /// Layer index.
        layer: usize,
        /// Why it is rejected.
        why: String,
    },
    /// Building the material / shape design failed.
    #[error(transparent)]
    Design(#[from] SSensitivityError),
}

/// Geometry of the flat unit cell and its copper patch (mm).
#[derive(Debug, Clone, Copy)]
pub struct ElementGeometry {
    /// Cell periods `[Px, Py]`; the cell is `[0, Px] × [0, Py]`.
    pub period: [f64; 2],
    /// `z` extent of the cell `[z0, z1]` (the Floquet ports).
    pub z_range: [f64; 2],
    /// `z` of the copper plane, strictly inside `z_range`.
    pub z_copper: f64,
    /// Base patch `[xl, xr]`.
    pub patch_x: [f64; 2],
    /// Base patch `[yl, yh]`.
    pub patch_y: [f64; 2],
}

/// Samples of `(value, ∂value/∂θ)`.
type Samples = Vec<(f64, Vec<f64>)>;

/// Edge index: right (`+x`), left (`−x`), top (`+y`), bottom (`−y`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Right,
    Left,
    Top,
    Bottom,
}

const EDGES: [Edge; 4] = [Edge::Right, Edge::Left, Edge::Top, Edge::Bottom];

/// The parameterization. See the module docs.
#[derive(Debug, Clone)]
pub struct FloquetElementParam {
    geo: ElementGeometry,
    layers: Vec<StackLayer>,
    n_edge: usize,
    n_samples: usize,
    beta: f64,
    spline_x: BSpline1D,
    spline_y: BSpline1D,
}

/// Constraint values and Jacobian (`jac[i][j] = ∂g_i/∂θ_j`).
#[derive(Debug, Clone)]
pub struct ElementConstraints {
    /// `g(θ)`, feasible when every entry is `≤ 0`.
    pub g: Vec<f64>,
    /// Row-major Jacobian.
    pub jac: Vec<Vec<f64>>,
}

impl ElementConstraints {
    /// Largest constraint value; `> 0` means the design is flagged.
    pub fn max_violation(&self) -> f64 {
        self.g.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    /// Whether any constraint is violated (`g > 0`).
    pub fn is_violated(&self) -> bool {
        self.max_violation() > 0.0
    }
}

impl FloquetElementParam {
    /// Build the parameterization.
    ///
    /// `n_edge` coefficients per edge (so `4·n_edge` outline parameters; use
    /// 12–49 for 50–200 variables), `n_samples ≥ 2` constraint samples per
    /// edge, KS sharpness `beta` (1/mm).
    ///
    /// # Errors
    ///
    /// [`FloquetElementError::Invalid`] for an inconsistent cell,
    /// [`FloquetElementError::NonStandardLayer`] for a layer that is not in
    /// [`STANDARD_STACK`] at a stock thickness.
    pub fn new(
        geo: ElementGeometry,
        layers: Vec<StackLayer>,
        n_edge: usize,
        n_samples: usize,
        beta: f64,
    ) -> Result<Self, FloquetElementError> {
        let bad = |s: &str| Err(FloquetElementError::Invalid(s.into()));
        let [px, py] = geo.period;
        let [xl, xr] = geo.patch_x;
        let [yl, yh] = geo.patch_y;
        let [z0, z1] = geo.z_range;
        if !(px > 0.0 && py > 0.0) {
            return bad("periods must be positive");
        }
        if !(0.0 < xl && xl < xr && xr < px && 0.0 < yl && yl < yh && yh < py) {
            return bad("patch must lie strictly inside the cell");
        }
        if !(z0 < geo.z_copper && geo.z_copper < z1) {
            return bad("z_copper must lie strictly inside z_range");
        }
        if n_edge == 0 || n_samples < 2 || beta <= 0.0 {
            return bad("need n_edge ≥ 1, n_samples ≥ 2, beta > 0");
        }
        for (i, l) in layers.iter().enumerate() {
            let m = STANDARD_STACK.get(l.material).ok_or_else(|| {
                FloquetElementError::NonStandardLayer {
                    layer: i,
                    why: format!("material index {} is not in the standard table", l.material),
                }
            })?;
            if !m
                .thicknesses_mm
                .iter()
                .any(|t| (t - l.thickness_mm).abs() < 1e-9)
            {
                return Err(FloquetElementError::NonStandardLayer {
                    layer: i,
                    why: format!(
                        "{} thickness {} mm is not a stock thickness {:?}",
                        m.name, l.thickness_mm, m.thicknesses_mm
                    ),
                });
            }
        }
        Ok(Self {
            geo,
            layers,
            n_edge,
            n_samples,
            beta,
            spline_x: BSpline1D::uniform_clamped(n_edge, 3, xl, xr),
            spline_y: BSpline1D::uniform_clamped(n_edge, 3, yl, yh),
        })
    }

    /// Number of design variables `L + 4·n_edge`.
    pub fn n_params(&self) -> usize {
        self.layers.len() + 4 * self.n_edge
    }

    /// Index of the first outline parameter (after the `ε′` block).
    pub fn outline_offset(&self) -> usize {
        self.layers.len()
    }

    /// `θ₀`: layer `ε′` at the midpoint of each material's range, zero
    /// outline displacement.
    pub fn initial_theta(&self) -> Vec<f64> {
        let mut th: Vec<f64> = self
            .layers
            .iter()
            .map(|l| {
                let m = &STANDARD_STACK[l.material];
                0.5 * (m.eps_min + m.eps_max)
            })
            .collect();
        th.resize(self.n_params(), 0.0);
        th
    }

    /// KS tolerance `ln(N)/β` (mm) of [`Self::constraints`], with `N` the
    /// number of samples per edge.
    pub fn ks_tolerance(&self) -> f64 {
        (self.n_samples as f64).ln() / self.beta
    }

    /// Outward displacement of `edge` at edge coordinate `t` for
    /// coefficients `c`, and the basis values (its gradient w.r.t. `c`).
    fn edge_basis(&self, edge: Edge, t: f64) -> Vec<f64> {
        match edge {
            Edge::Right | Edge::Left => self.spline_y.eval(t),
            Edge::Top | Edge::Bottom => self.spline_x.eval(t),
        }
    }

    fn coeffs<'a>(&self, theta: &'a [f64], edge: Edge) -> &'a [f64] {
        let e = EDGES.iter().position(|&x| x == edge).unwrap();
        let o = self.outline_offset() + e * self.n_edge;
        &theta[o..o + self.n_edge]
    }

    /// Check `theta` length.
    fn check(&self, theta: &[f64]) -> Result<(), FloquetElementError> {
        if theta.len() == self.n_params() {
            Ok(())
        } else {
            Err(FloquetElementError::Invalid(format!(
                "θ has {} entries, expected {}",
                theta.len(),
                self.n_params()
            )))
        }
    }

    /// Smooth edge-normal taper: 0 at the patch centre `c`, 1 at the edge
    /// `e`, 0 at the cell wall `wall` (the wall lies beyond `e`).
    fn normal_taper(u: f64, c: f64, e: f64, wall: f64) -> f64 {
        // Orient so that the edge is at larger coordinate than the centre.
        let s = (wall - e).signum();
        let (u, c, e, wall) = (s * u, s * c, s * e, s * wall);
        if u <= c {
            0.0
        } else if u <= e {
            smoothstep((u - c) / (e - c))
        } else {
            1.0 - smoothstep((u - e) / (wall - e))
        }
    }

    /// Smooth along-edge envelope: 1 on `[lo, hi]`, 0 at `0` and at `period`.
    fn along_envelope(t: f64, lo: f64, hi: f64, period: f64) -> f64 {
        if t < lo {
            smoothstep(t / lo)
        } else if t > hi {
            1.0 - smoothstep((t - hi) / (period - hi))
        } else {
            1.0
        }
    }

    fn z_taper(&self, z: f64) -> f64 {
        let [z0, z1] = self.geo.z_range;
        let zc = self.geo.z_copper;
        if z <= zc {
            smoothstep((z - z0) / (zc - z0))
        } else {
            1.0 - smoothstep((z - zc) / (z1 - zc))
        }
    }

    /// Node-motion columns `∂X/∂θ_outline` over `mesh`, one per outline
    /// parameter, ordered right, left, top, bottom.
    pub fn outline_columns(&self, mesh: &TetMesh) -> Vec<Vec<[f64; 3]>> {
        let g = &self.geo;
        let [px, py] = g.period;
        let [xl, xr] = g.patch_x;
        let [yl, yh] = g.patch_y;
        let cx = 0.5 * (xl + xr);
        let cy = 0.5 * (yl + yh);
        let mut cols = Vec::with_capacity(4 * self.n_edge);
        for edge in EDGES {
            // Per node: (taper·envelope·z, edge coordinate).
            let per_node: Vec<(f64, f64)> = mesh
                .nodes
                .iter()
                .map(|p| {
                    let (normal, along) = match edge {
                        Edge::Right => (
                            Self::normal_taper(p[0], cx, xr, px),
                            Self::along_envelope(p[1], yl, yh, py),
                        ),
                        Edge::Left => (
                            Self::normal_taper(p[0], cx, xl, 0.0),
                            Self::along_envelope(p[1], yl, yh, py),
                        ),
                        Edge::Top => (
                            Self::normal_taper(p[1], cy, yh, py),
                            Self::along_envelope(p[0], xl, xr, px),
                        ),
                        Edge::Bottom => (
                            Self::normal_taper(p[1], cy, yl, 0.0),
                            Self::along_envelope(p[0], xl, xr, px),
                        ),
                    };
                    let t = match edge {
                        Edge::Right | Edge::Left => p[1],
                        _ => p[0],
                    };
                    (normal * along * self.z_taper(p[2]), t)
                })
                .collect();
            let basis: Vec<Vec<f64>> = per_node
                .iter()
                .map(|&(_, t)| self.edge_basis(edge, t))
                .collect();
            let dir = match edge {
                Edge::Right => [1.0, 0.0, 0.0],
                Edge::Left => [-1.0, 0.0, 0.0],
                Edge::Top => [0.0, 1.0, 0.0],
                Edge::Bottom => [0.0, -1.0, 0.0],
            };
            for i in 0..self.n_edge {
                cols.push(
                    per_node
                        .iter()
                        .zip(&basis)
                        .map(|(&(w, _), b)| {
                            let a = w * b[i];
                            [a * dir[0], a * dir[1], a * dir[2]]
                        })
                        .collect(),
                );
            }
        }
        cols
    }

    /// The adjoint's design: one `ε′` region per layer (`layer_of_tet[t]`
    /// is the layer of tet `t`, `None` outside the superstrate) and the
    /// outline columns. Parameter order of the adjoint output equals the
    /// order of `θ`.
    ///
    /// # Errors
    ///
    /// [`FloquetElementError::Design`] if a layer has no tet or a label is
    /// out of range.
    pub fn design(
        &self,
        mesh: &TetMesh,
        layer_of_tet: Vec<Option<usize>>,
    ) -> Result<FloquetDesign, FloquetElementError> {
        let names: Vec<String> = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| format!("layer{i}_{}", STANDARD_STACK[l.material].name))
            .collect();
        let mut shape_names = Vec::new();
        for e in ["right", "left", "top", "bottom"] {
            for i in 0..self.n_edge {
                shape_names.push(format!("{e}{i}"));
            }
        }
        let material = if self.layers.is_empty() {
            None
        } else {
            Some(MaterialDesign::from_regions(layer_of_tet, names)?.with_components(true, false))
        };
        Ok(FloquetDesign {
            material,
            shape: Some(ShapeDesign::from_columns(
                self.outline_columns(mesh),
                shape_names,
            )?),
        })
    }

    /// Patch edge position after displacement: `(xl, xr, yl, yh)` at
    /// coordinate samples; used by the constraints. Returns the widths
    /// `w_x(y_k)` and `w_y(x_k)` together with their rows
    /// `∂w/∂θ` (`n_params` long).
    fn width_samples(&self, theta: &[f64]) -> (Samples, Samples) {
        let n = self.n_params();
        let off = self.outline_offset();
        let ne = self.n_edge;
        let mk = |spline: &BSpline1D, lo: f64, hi: f64, w0: f64, a: Edge, b: Edge| {
            let (ca, cb) = (self.coeffs(theta, a), self.coeffs(theta, b));
            let ia = off + EDGES.iter().position(|&x| x == a).unwrap() * ne;
            let ib = off + EDGES.iter().position(|&x| x == b).unwrap() * ne;
            (0..self.n_samples)
                .map(|k| {
                    let t = lo + (hi - lo) * k as f64 / (self.n_samples - 1) as f64;
                    let basis = spline.eval(t);
                    let mut row = vec![0.0; n];
                    let mut w = w0;
                    for i in 0..ne {
                        w += basis[i] * (ca[i] + cb[i]);
                        row[ia + i] = basis[i];
                        row[ib + i] = basis[i];
                    }
                    (w, row)
                })
                .collect::<Vec<_>>()
        };
        let [xl, xr] = self.geo.patch_x;
        let [yl, yh] = self.geo.patch_y;
        (
            mk(&self.spline_y, yl, yh, xr - xl, Edge::Right, Edge::Left),
            mk(&self.spline_x, xl, xr, yh - yl, Edge::Top, Edge::Bottom),
        )
    }

    /// Every constraint individually (exact, `g = 0` at exactly the limit):
    /// trace x, trace y, gap x, gap y samples, then the layer `ε′` bounds
    /// `[ε_min − ε′, ε′ − ε_max]` per layer.
    ///
    /// # Errors
    ///
    /// [`FloquetElementError::Invalid`] on a wrong `θ` length.
    pub fn constraints_sampled(
        &self,
        theta: &[f64],
    ) -> Result<ElementConstraints, FloquetElementError> {
        let groups = self.groups(theta)?;
        let mut g = Vec::new();
        let mut jac = Vec::new();
        for grp in groups.into_iter().flatten() {
            g.push(grp.0);
            jac.push(grp.1);
        }
        let (lg, lj) = self.layer_rows(theta);
        g.extend(lg);
        jac.extend(lj);
        Ok(ElementConstraints { g, jac })
    }

    /// Smooth aggregate for MMA: four KS-folded geometric constraints
    /// (trace x, trace y, gap x, gap y), then the `2·L` layer bounds.
    ///
    /// # Errors
    ///
    /// [`FloquetElementError::Invalid`] on a wrong `θ` length.
    pub fn constraints(&self, theta: &[f64]) -> Result<ElementConstraints, FloquetElementError> {
        let groups = self.groups(theta)?;
        let mut g = Vec::new();
        let mut jac = Vec::new();
        for grp in groups {
            let m = grp.iter().map(|s| s.0).fold(f64::NEG_INFINITY, f64::max);
            let wts: Vec<f64> = grp.iter().map(|s| (self.beta * (s.0 - m)).exp()).collect();
            let sum: f64 = wts.iter().sum();
            g.push(m + sum.ln() / self.beta);
            let mut row = vec![0.0; self.n_params()];
            for (w, s) in wts.iter().zip(&grp) {
                for (r, d) in row.iter_mut().zip(&s.1) {
                    *r += w / sum * d;
                }
            }
            jac.push(row);
        }
        let (lg, lj) = self.layer_rows(theta);
        g.extend(lg);
        jac.extend(lj);
        Ok(ElementConstraints { g, jac })
    }

    /// `[trace_x, trace_y, gap_x, gap_y]` sample groups of `(g, ∂g/∂θ)`.
    fn groups(&self, theta: &[f64]) -> Result<[Samples; 4], FloquetElementError> {
        self.check(theta)?;
        let (wx, wy) = self.width_samples(theta);
        let [px, py] = self.geo.period;
        let neg = |row: &[f64]| row.iter().map(|v| -v).collect::<Vec<_>>();
        let trace = |ws: &[(f64, Vec<f64>)]| {
            ws.iter()
                .map(|(w, r)| (MIN_TRACE_MM - w, neg(r)))
                .collect::<Vec<_>>()
        };
        let gap = |ws: &[(f64, Vec<f64>)], p: f64| {
            ws.iter()
                .map(|(w, r)| (MIN_GAP_MM - (p - w), r.clone()))
                .collect::<Vec<_>>()
        };
        Ok([trace(&wx), trace(&wy), gap(&wx, px), gap(&wy, py)])
    }

    fn layer_rows(&self, theta: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
        let mut g = Vec::new();
        let mut jac = Vec::new();
        for (i, l) in self.layers.iter().enumerate() {
            let m = &STANDARD_STACK[l.material];
            let mut lo = vec![0.0; self.n_params()];
            lo[i] = -1.0;
            let mut hi = vec![0.0; self.n_params()];
            hi[i] = 1.0;
            g.push(m.eps_min - theta[i]);
            jac.push(lo);
            g.push(theta[i] - m.eps_max);
            jac.push(hi);
        }
        (g, jac)
    }
}
