//! **ICEM reference motor: design parametrization as fixed-topology node
//! motion** (motor paper M3a, issue #1173).
//!
//! The ICEM reference optimization (`benchmarks/motor_reference/`) varies 9
//! geometric parameters of the 60° PMSM sector: the magnet (`MAG`, `MH`,
//! `MW`) and the stator slot (`SW1`, `SW2`, `SW3`, `SW4`, `SD1`, `Sr1`).
//! This module gives the map `θ ↦ X(θ)` from those parameters to the node
//! coordinates of a mesh generated **once** at `θ0`, together with its exact
//! Jacobian `∂X/∂θ`, for the shape adjoint (M3b) and the optimizer (M4/M5).
//!
//! # Construction (written for GEODE)
//!
//! [`sector_curves`] builds every material interface and domain boundary of
//! the sector as exact lines and circular arcs in the lab frame. The
//! construction was derived here from the parameter definitions; the
//! upstream GPL generators were read only to learn what each parameter
//! means. It is checked against the upstream NURBS exports in
//! `tests/motor_icem_parametric.rs` and
//! `benchmarks/motor_reference/parametric/`.
//!
//! *Rotor* (pole axis along +x in the rotor frame, then turned by +90° so
//! the sector is `[60°, 120°]`): the magnet is the rectangle
//! `x ∈ [R_r − MAG − MH, R_r − MAG]`, `|y| ≤ MW/2`, with `R_r = 44 mm` the
//! rotor radius, so `MAG` is the iron depth between the magnet's outer face
//! and the rotor surface on the pole axis. Each flux-barrier pocket is the
//! quadrilateral spanned by the two magnet corners on its side and two fixed
//! points: `r = R_r − 3 mm` at `±0.37558 rad` and `r = R_r − 2 mm` at
//! `±0.37 rad`. The shaft (`16 mm`), the rotor surface, the radial sector
//! edges and the coupling circle (`r_c = 44.5 mm`) do not depend on `θ`.
//!
//! *Stator slot* (slot axis along +x in the slot frame; slot `k = 0..5` is
//! turned by `65° + 10° k`; tooth centrelines at `±α`, `α = 5°`):
//! - the bore is the circle `R_b = 45 mm`; the slot opening has width `SW2`,
//!   so its corners sit on the bore at `±asin(SW2 / (2 R_b))`;
//! - the opening is a straight channel of width `SW2` and depth `SW3`,
//!   measured from the chord of the bore corners to its inner end `C±`;
//! - the tooth has constant width `SW1`, so each slot flank is the line
//!   parallel to its tooth centreline at distance `SW1/2`;
//! - the slot top is a circular arc through `C±`, tangent to both flanks.
//!   With centre `(u, 0)` and radius `ρ`, tangency gives `ρ = u sin α − SW1/2`
//!   and the passage through `C±` gives `(u − x_c)² + (SW2/2)² = ρ²`, a
//!   quadratic in `u`; the larger root is the arc on the slot side;
//! - the slot bottom is the straight line `x = SD1/2 − SW4` (back-iron depth
//!   `SW4` on the slot axis); each bottom corner is a fillet of radius `Sr1`
//!   tangent to the flank and to the bottom;
//! - the two coil sides (winding groups) are divided by the chord between the
//!   midpoints of the two straight flank segments;
//! - the outer boundary is the circle `SD1/2`.
//!
//! # Node motion
//!
//! [`IcemShapeMap::new`] attaches every node of the `θ0` mesh that lies on
//! one of these curves to it: a curve end point keeps its identity, and an
//! interior node keeps its parameter (arc-length fraction on a line, angle
//! fraction on an arc). At `θ` such a node sits at the moved curve's point
//! with the same parameter. All other nodes are interior to one material
//! region. Their displacement is the discrete harmonic extension of the
//! boundary displacement: the P1 Laplacian of the `θ0` mesh, by default with
//! Jacobian stiffening (element weight `1/A_e`, [`MotionOptions`]). Because
//! every material interface is a Dirichlet curve, the extension decouples
//! by region. One exception to the curve rule: on the stator's radial sector
//! edges (bore to outer circle) each antiperiodic node pair shares one
//! radial displacement that the extension solves for ([`EdgeRule::Slide`]),
//! so the pair stays an exact `R(60°)` image without forcing a stretch onto
//! the iron next to the edge. The map is
//!
//! ```text
//!   X(θ) = X0 + E · (X_b(θ) − X_b(θ0)),      ∂X/∂θ = E · ∂X_b/∂θ,
//! ```
//!
//! with `E` linear and fixed, and `∂X_b/∂θ` from forward-mode dual numbers
//! ([`Dual`]) through [`sector_curves`]. The Jacobian is exact for this map.
//!
//! **Air gap.** The shaft, rotor surface, coupling circle, rotor radial edges,
//! and the gap-band parts of the stator radial edges are independent of `θ`,
//! so the inner gap band (tag 4) and both coupling circles (tags 121, 122)
//! never move. The rotation and sliding-band logic sees the original nodes
//! there. The outer gap band (tag 5) keeps its shape as a set (`r_c` to
//! `R_b`), but `SW2` slides the slot-opening corners along the bore, and the
//! band's bore nodes and interior nodes follow. No other parameter moves
//! tag 5.

use std::f64::consts::PI;
use std::ops::{Add, Div, Mul, Neg, Sub};

use faer::Mat;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, Triplet};

use super::tagged_tri::TaggedTriMesh;
use crate::analytic::waveguide::TriMesh;

/// Number of geometric design parameters.
pub const N_PARAMS: usize = 9;

/// Parameter names in the order of [`IcemDesign::values`]. This is the
/// order in which they appear in `reference_data.json`
/// `optimization_parameter_bounds`, after `ScaleR`, `LENGTH` and
/// `OPERATING_ANGLE`, which are not geometric.
pub const PARAM_NAMES: [&str; N_PARAMS] =
    ["MAG", "MH", "MW", "SW1", "SW2", "SW3", "SW4", "SD1", "Sr1"];

/// Shaft radius (m), `RD2/2`.
pub const R_SHAFT: f64 = 0.016;
/// Rotor radius (m), `RD1/2`.
pub const R_ROTOR: f64 = 0.044;
/// Coupling-circle radius (m), midway across the gap.
pub const R_COUPLING: f64 = 0.0445;
/// Stator bore radius (m), `SD2/2`.
pub const R_BORE: f64 = 0.045;
/// Radial depth (m) of the inner pocket corner below the rotor surface (`LW2`).
pub const POCKET_DEPTH_INNER: f64 = 0.003;
/// Radial depth (m) of the outer pocket corner below the rotor surface (`LW1`).
pub const POCKET_DEPTH_OUTER: f64 = 0.002;
/// Angle (rad, from the pole axis) of the inner pocket corner (`Theta2`).
pub const POCKET_ANGLE_INNER: f64 = 0.37558;
/// Angle (rad, from the pole axis) of the outer pocket corner (`Theta1`).
pub const POCKET_ANGLE_OUTER: f64 = 0.37;
/// Half pole pitch (rad): 6 poles.
pub const HALF_POLE: f64 = PI / 6.0;
/// Half slot pitch (rad): 36 slots.
pub const HALF_SLOT: f64 = PI / 36.0;
/// Slots in the modelled 60° sector.
pub const SLOTS_IN_SECTOR: usize = 6;
/// Lab angle of the rotor pole axis at rotor angle 0.
pub const ROTOR_FRAME: f64 = PI / 2.0;
/// Lab angle of slot 0's axis (65°); slot `k` is at `65° + 10° k`.
pub const STATOR_FRAME0: f64 = 13.0 * PI / 36.0;
/// Sector edges (lab, rad).
pub const SECTOR_LO: f64 = PI / 3.0;
/// Upper sector edge (lab, rad).
pub const SECTOR_HI: f64 = 2.0 * PI / 3.0;

/// A point of the 9-parameter design space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IcemDesign {
    /// `[MAG, MH, MW, SW1, SW2, SW3, SW4, SD1, Sr1]` in metres
    /// ([`PARAM_NAMES`] order).
    pub values: [f64; N_PARAMS],
}

impl IcemDesign {
    /// The optimizer's initial design (history iteration 0).
    pub const INITIAL: IcemDesign = IcemDesign {
        values: [
            0.007, 0.007, 0.019, 0.004, 0.0023, 0.001, 0.00825, 0.135, 0.001,
        ],
    };

    /// Index of parameter `name` in [`PARAM_NAMES`].
    pub fn index_of(name: &str) -> Option<usize> {
        PARAM_NAMES.iter().position(|n| *n == name)
    }

    /// Parameter `name`.
    pub fn get(&self, name: &str) -> Option<f64> {
        Self::index_of(name).map(|i| self.values[i])
    }
}

/// Errors of the parametrization and the node motion.
#[derive(Debug, thiserror::Error)]
pub enum IcemParamError {
    /// The parameters do not define a geometry (no tangent slot-top arc, a
    /// degenerate flank, ...).
    #[error("construction: {0}")]
    Construction(String),
    /// A mesh node on a material interface or boundary lies on no curve, or
    /// the mesh does not match the construction at `θ0`.
    #[error("classification: {0}")]
    Classification(String),
    /// Sparse assembly or factorization of the extension operator failed.
    #[error("solve: {0}")]
    Solve(String),
    /// The MSH text could not be rewritten.
    #[error("msh: {0}")]
    Msh(String),
}

// ── Scalars: f64 and forward-mode duals ─────────────────────────────────

/// The scalar operations [`sector_curves`] needs. Implemented by `f64` and by
/// the forward-mode [`Dual`], which carries `∂/∂θ` for all 9 parameters.
pub trait Real:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// A constant.
    fn cst(x: f64) -> Self;
    /// The value.
    fn value(self) -> f64;
    /// Square root.
    fn sqrt(self) -> Self;
    /// Sine.
    fn sin(self) -> Self;
    /// Cosine.
    fn cos(self) -> Self;
    /// Arcsine.
    fn asin(self) -> Self;
    /// `atan2(self, x)`.
    fn atan2(self, x: Self) -> Self;
}

impl Real for f64 {
    fn cst(x: f64) -> Self {
        x
    }
    fn value(self) -> f64 {
        self
    }
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
    fn sin(self) -> Self {
        f64::sin(self)
    }
    fn cos(self) -> Self {
        f64::cos(self)
    }
    fn asin(self) -> Self {
        f64::asin(self)
    }
    fn atan2(self, x: Self) -> Self {
        f64::atan2(self, x)
    }
}

/// Forward-mode dual number: a value and its gradient with respect to the 9
/// design parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual {
    /// Value.
    pub v: f64,
    /// `∂v/∂θ_k`, `k` in [`PARAM_NAMES`] order.
    pub d: [f64; N_PARAMS],
}

impl Dual {
    /// The independent variable `θ_k` with value `v`.
    pub fn var(v: f64, k: usize) -> Self {
        let mut d = [0.0; N_PARAMS];
        d[k] = 1.0;
        Self { v, d }
    }

    fn chain(self, v: f64, dv: f64) -> Self {
        Self {
            v,
            d: self.d.map(|x| x * dv),
        }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        let mut d = self.d;
        for (a, b) in d.iter_mut().zip(o.d) {
            *a += b;
        }
        Self { v: self.v + o.v, d }
    }
}

impl Sub for Dual {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        let mut d = self.d;
        for (a, b) in d.iter_mut().zip(o.d) {
            *a -= b;
        }
        Self { v: self.v - o.v, d }
    }
}

impl Mul for Dual {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let mut d = [0.0; N_PARAMS];
        for (k, dk) in d.iter_mut().enumerate() {
            *dk = self.d[k] * o.v + self.v * o.d[k];
        }
        Self { v: self.v * o.v, d }
    }
}

impl Div for Dual {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        let v = self.v / o.v;
        let mut d = [0.0; N_PARAMS];
        for (k, dk) in d.iter_mut().enumerate() {
            *dk = (self.d[k] - v * o.d[k]) / o.v;
        }
        Self { v, d }
    }
}

impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            v: -self.v,
            d: self.d.map(|x| -x),
        }
    }
}

impl Real for Dual {
    fn cst(x: f64) -> Self {
        Self {
            v: x,
            d: [0.0; N_PARAMS],
        }
    }
    fn value(self) -> f64 {
        self.v
    }
    fn sqrt(self) -> Self {
        let s = self.v.sqrt();
        self.chain(s, 0.5 / s)
    }
    fn sin(self) -> Self {
        self.chain(self.v.sin(), self.v.cos())
    }
    fn cos(self) -> Self {
        self.chain(self.v.cos(), -self.v.sin())
    }
    fn asin(self) -> Self {
        self.chain(self.v.asin(), 1.0 / (1.0 - self.v * self.v).sqrt())
    }
    fn atan2(self, x: Self) -> Self {
        let r2 = x.v * x.v + self.v * self.v;
        let mut d = [0.0; N_PARAMS];
        for (k, dk) in d.iter_mut().enumerate() {
            *dk = (x.v * self.d[k] - self.v * x.d[k]) / r2;
        }
        Self {
            v: self.v.atan2(x.v),
            d,
        }
    }
}

// ── Curves ──────────────────────────────────────────────────────────────

/// Shape of one boundary or interface curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveShape<T> {
    /// Segment from `a` to `b`.
    Line {
        /// Start point.
        a: [T; 2],
        /// End point.
        b: [T; 2],
    },
    /// Circular arc around `c` with radius `r` from angle `phi_a` to `phi_b`
    /// (`|phi_b − phi_a| < π`; either sense). `a` and `b` are the end points
    /// as constructed (equal to the arc's end points up to round-off).
    Arc {
        /// Centre.
        c: [T; 2],
        /// Radius.
        r: T,
        /// Start angle (rad).
        phi_a: T,
        /// End angle (rad).
        phi_b: T,
        /// Start point.
        a: [T; 2],
        /// End point.
        b: [T; 2],
    },
}

/// A named curve of the construction.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve<T> {
    /// Stable name, e.g. `rotor.magnet_outer` or `slot3.tip_hi`.
    pub name: String,
    /// Geometry.
    pub shape: CurveShape<T>,
}

impl<T: Real> Curve<T> {
    /// Start point.
    pub fn start(&self) -> [T; 2] {
        match self.shape {
            CurveShape::Line { a, .. } | CurveShape::Arc { a, .. } => a,
        }
    }

    /// End point.
    pub fn end(&self) -> [T; 2] {
        match self.shape {
            CurveShape::Line { b, .. } | CurveShape::Arc { b, .. } => b,
        }
    }

    /// Point at parameter `t ∈ [0, 1]` (fraction of length on a line, of
    /// angle on an arc). `t = 0` and `t = 1` return the constructed end
    /// points exactly.
    pub fn point(&self, t: f64) -> [T; 2] {
        if t == 0.0 {
            return self.start();
        }
        if t == 1.0 {
            return self.end();
        }
        let tt = T::cst(t);
        match self.shape {
            CurveShape::Line { a, b } => [a[0] + tt * (b[0] - a[0]), a[1] + tt * (b[1] - a[1])],
            CurveShape::Arc {
                c, r, phi_a, phi_b, ..
            } => {
                let phi = phi_a + tt * (phi_b - phi_a);
                [c[0] + r * phi.cos(), c[1] + r * phi.sin()]
            }
        }
    }
}

impl Curve<f64> {
    /// `(distance, t)` of `p` to the curve, with `t` the parameter of the
    /// closest point when it lies within the curve (`t ∈ [0, 1]` up to
    /// `1e-9`); `None` when the projection falls outside it.
    pub fn locate(&self, p: [f64; 2]) -> Option<(f64, f64)> {
        const T_SLACK: f64 = 1e-9;
        match self.shape {
            CurveShape::Line { a, b } => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = dx * dx + dy * dy;
                let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2;
                if !(-T_SLACK..=1.0 + T_SLACK).contains(&t) {
                    return None;
                }
                let t = t.clamp(0.0, 1.0);
                let q = [a[0] + t * dx, a[1] + t * dy];
                Some(((p[0] - q[0]).hypot(p[1] - q[1]), t))
            }
            CurveShape::Arc {
                c, r, phi_a, phi_b, ..
            } => {
                let mut phi = (p[1] - c[1]).atan2(p[0] - c[0]);
                let mid = 0.5 * (phi_a + phi_b);
                while phi - mid > PI {
                    phi -= 2.0 * PI;
                }
                while phi - mid < -PI {
                    phi += 2.0 * PI;
                }
                let t = (phi - phi_a) / (phi_b - phi_a);
                if !(-T_SLACK..=1.0 + T_SLACK).contains(&t) {
                    return None;
                }
                let dist = ((p[0] - c[0]).hypot(p[1] - c[1]) - r).abs();
                Some((dist, t.clamp(0.0, 1.0)))
            }
        }
    }

    /// Distance of `p` to the curve (to the nearest end point when the
    /// projection falls outside).
    pub fn distance(&self, p: [f64; 2]) -> f64 {
        let ends = [self.start(), self.end()]
            .iter()
            .map(|q| (p[0] - q[0]).hypot(p[1] - q[1]))
            .fold(f64::INFINITY, f64::min);
        match self.locate(p) {
            Some((d, _)) => d.min(ends),
            None => ends,
        }
    }
}

fn rot<T: Real>(p: [T; 2], angle: f64) -> [T; 2] {
    let (c, s) = (T::cst(angle.cos()), T::cst(angle.sin()));
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

fn polar<T: Real>(r: T, phi: T) -> [T; 2] {
    [r * phi.cos(), r * phi.sin()]
}

fn line<T: Real>(name: String, a: [T; 2], b: [T; 2]) -> Curve<T> {
    Curve {
        name,
        shape: CurveShape::Line { a, b },
    }
}

#[allow(clippy::too_many_arguments)]
fn arc<T: Real>(
    name: String,
    c: [T; 2],
    r: T,
    phi_a: T,
    phi_b: T,
    a: [T; 2],
    b: [T; 2],
) -> Curve<T> {
    Curve {
        name,
        shape: CurveShape::Arc {
            c,
            r,
            phi_a,
            phi_b,
            a,
            b,
        },
    }
}

/// Origin-centred arc of radius `r` between angles, with polar end points.
fn circle_arc<T: Real>(name: String, r: T, phi_a: T, phi_b: T) -> Curve<T> {
    let z = T::cst(0.0);
    arc(
        name,
        [z, z],
        r,
        phi_a,
        phi_b,
        polar(r, phi_a),
        polar(r, phi_b),
    )
}

fn rotate_curve<T: Real>(c: Curve<T>, angle: f64) -> Curve<T> {
    let shape = match c.shape {
        CurveShape::Line { a, b } => CurveShape::Line {
            a: rot(a, angle),
            b: rot(b, angle),
        },
        CurveShape::Arc {
            c: cc,
            r,
            phi_a,
            phi_b,
            a,
            b,
        } => CurveShape::Arc {
            c: rot(cc, angle),
            r,
            phi_a: phi_a + T::cst(angle),
            phi_b: phi_b + T::cst(angle),
            a: rot(a, angle),
            b: rot(b, angle),
        },
    };
    Curve {
        name: c.name,
        shape,
    }
}

fn mirror<T: Real>(p: [T; 2]) -> [T; 2] {
    [p[0], -p[1]]
}

/// Every material interface and domain boundary of the 60° sector at
/// parameters `p` (lab frame, rotor angle 0), as named lines and arcs.
///
/// Rotor curves are prefixed `rotor.`, slot `k` curves `slot{k}.`, and the
/// remaining stator curves `stator.`. See the module documentation for the
/// construction.
///
/// # Errors
///
/// [`IcemParamError::Construction`] if the slot top has no tangent arc
/// through the opening corners, or a flank segment, fillet or pocket
/// degenerates.
pub fn sector_curves<T: Real>(p: &[T; N_PARAMS]) -> Result<Vec<Curve<T>>, IcemParamError> {
    let c = T::cst;
    let half = c(0.5);
    let [mag, mh, mw, sw1, sw2, sw3, sw4, sd1, sr1] = *p;
    let mut out = Vec::with_capacity(16 + 15 * SLOTS_IN_SECTOR + 7);

    // ---- rotor (pole axis +x), turned by ROTOR_FRAME ----
    let x_in = c(R_ROTOR) - mag - mh;
    let x_out = c(R_ROTOR) - mag;
    let w = mw * half;
    let m_in_lo = [x_in, -w];
    let m_in_hi = [x_in, w];
    let m_out_lo = [x_out, -w];
    let m_out_hi = [x_out, w];
    if !(x_in.value() > R_SHAFT && mh.value() > 0.0 && w.value() > 0.0) {
        return Err(IcemParamError::Construction(format!(
            "magnet inner face x = {:.6e} m, MH = {:.3e}, MW = {:.3e}",
            x_in.value(),
            mh.value(),
            mw.value()
        )));
    }
    let r_pi = c(R_ROTOR - POCKET_DEPTH_INNER);
    let r_po = c(R_ROTOR - POCKET_DEPTH_OUTER);
    let q_in_lo = polar(r_pi, c(-POCKET_ANGLE_INNER));
    let q_out_lo = polar(r_po, c(-POCKET_ANGLE_OUTER));
    let rotor_local = vec![
        line("rotor.magnet_inner".into(), m_in_lo, m_in_hi),
        line("rotor.magnet_outer".into(), m_out_lo, m_out_hi),
        line("rotor.magnet_side_lo".into(), m_in_lo, m_out_lo),
        line("rotor.magnet_side_hi".into(), m_in_hi, m_out_hi),
        line("rotor.pocket_lo_inner".into(), q_in_lo, m_in_lo),
        line("rotor.pocket_lo_outer".into(), m_out_lo, q_out_lo),
        line("rotor.pocket_lo_bridge".into(), q_out_lo, q_in_lo),
        line("rotor.pocket_hi_inner".into(), mirror(q_in_lo), m_in_hi),
        line("rotor.pocket_hi_outer".into(), m_out_hi, mirror(q_out_lo)),
        line(
            "rotor.pocket_hi_bridge".into(),
            mirror(q_out_lo),
            mirror(q_in_lo),
        ),
        circle_arc(
            "rotor.shaft".into(),
            c(R_SHAFT),
            c(-HALF_POLE),
            c(HALF_POLE),
        ),
        circle_arc(
            "rotor.surface".into(),
            c(R_ROTOR),
            c(-HALF_POLE),
            c(HALF_POLE),
        ),
        line(
            "rotor.edge_lo".into(),
            polar(c(R_SHAFT), c(-HALF_POLE)),
            polar(c(R_COUPLING), c(-HALF_POLE)),
        ),
        line(
            "rotor.edge_hi".into(),
            polar(c(R_SHAFT), c(HALF_POLE)),
            polar(c(R_COUPLING), c(HALF_POLE)),
        ),
    ];
    out.extend(
        rotor_local
            .into_iter()
            .map(|cv| rotate_curve(cv, ROTOR_FRAME)),
    );

    // ---- stator slot cell (slot axis +x) ----
    let alpha = HALF_SLOT;
    let (sa, ca) = (alpha.sin(), alpha.cos());
    let s = sw2 * half; // opening half-width
    let wt = sw1 * half; // tooth half-width
    if !(s.value() > 0.0 && s.value() < R_BORE * sa) {
        return Err(IcemParamError::Construction(format!(
            "slot opening SW2 = {:.4e} m does not fit the slot pitch",
            sw2.value()
        )));
    }
    let phi_o = (s / c(R_BORE)).asin();
    let o_hi = [c(R_BORE) * phi_o.cos(), s];
    let x_c = o_hi[0] + sw3;
    let c_hi = [x_c, s];
    // Tangent slot-top arc: u^2 cos^2 a - 2 u (x_c - wt sin a) + (x_c^2 + s^2 - wt^2) = 0.
    let qa = c(ca * ca);
    let qb = x_c - wt * c(sa);
    let qc = x_c * x_c + s * s - wt * wt;
    let disc = qb * qb - qa * qc;
    if disc.value().is_nan() || disc.value() < 0.0 {
        return Err(IcemParamError::Construction(format!(
            "no slot-top arc through the opening tangent to both flanks (discriminant {:.3e})",
            disc.value()
        )));
    }
    let u = (qb + disc.sqrt()) / qa;
    let rho = u * c(sa) - wt;
    if !(u.value() > x_c.value() && rho.value() > s.value()) {
        return Err(IcemParamError::Construction(format!(
            "slot-top arc centre {:.6e} m not beyond the opening end {:.6e} m (radius {:.3e})",
            u.value(),
            x_c.value(),
            rho.value()
        )));
    }
    let t_hi = [u - rho * c(sa), rho * c(ca)];
    let phi_c = s.atan2(x_c - u); // angle of C+ seen from the arc centre
    let phi_t = c(PI / 2.0 + alpha);
    let x_b = sd1 * half - sw4; // slot bottom
    let x_f = x_b - sr1; // fillet centre
    let y_f = (x_f * c(sa) - wt - sr1) / c(ca);
    let f_hi = [x_f - sr1 * c(sa), y_f + sr1 * c(ca)];
    let b_hi = [x_b, y_f];
    if !(sr1.value() > 0.0 && y_f.value() > 0.0 && f_hi[0].value() > t_hi[0].value()) {
        return Err(IcemParamError::Construction(format!(
            "slot flank degenerates: fillet centre y = {:.4e} m, flank from x = {:.6e} to {:.6e} m",
            y_f.value(),
            t_hi[0].value(),
            f_hi[0].value()
        )));
    }
    let m_hi = [(t_hi[0] + f_hi[0]) * half, (t_hi[1] + f_hi[1]) * half];
    let z = c(0.0);
    let ro = sd1 * half;
    for k in 0..SLOTS_IN_SECTOR {
        let pre = format!("slot{k}.");
        let cell = vec![
            circle_arc(format!("{pre}bore_lo"), c(R_BORE), c(-alpha), -phi_o),
            circle_arc(format!("{pre}bore_open"), c(R_BORE), -phi_o, phi_o),
            circle_arc(format!("{pre}bore_hi"), c(R_BORE), phi_o, c(alpha)),
            line(format!("{pre}channel_lo"), mirror(o_hi), mirror(c_hi)),
            line(format!("{pre}channel_hi"), o_hi, c_hi),
            line(format!("{pre}opening_end"), mirror(c_hi), c_hi),
            arc(
                format!("{pre}tip_lo"),
                [u, z],
                rho,
                -phi_c,
                -phi_t,
                mirror(c_hi),
                mirror(t_hi),
            ),
            arc(
                format!("{pre}tip_hi"),
                [u, z],
                rho,
                phi_c,
                phi_t,
                c_hi,
                t_hi,
            ),
            line(format!("{pre}flank_lo_in"), mirror(t_hi), mirror(m_hi)),
            line(format!("{pre}flank_lo_out"), mirror(m_hi), mirror(f_hi)),
            line(format!("{pre}flank_hi_in"), t_hi, m_hi),
            line(format!("{pre}flank_hi_out"), m_hi, f_hi),
            arc(
                format!("{pre}fillet_lo"),
                [x_f, -y_f],
                sr1,
                -phi_t,
                z,
                mirror(f_hi),
                mirror(b_hi),
            ),
            arc(
                format!("{pre}fillet_hi"),
                [x_f, y_f],
                sr1,
                phi_t,
                z,
                f_hi,
                b_hi,
            ),
            line(format!("{pre}bottom"), mirror(b_hi), b_hi),
            line(format!("{pre}coil_divider"), mirror(m_hi), m_hi),
        ];
        let angle = STATOR_FRAME0 + 2.0 * HALF_SLOT * k as f64;
        out.extend(cell.into_iter().map(|cv| rotate_curve(cv, angle)));
    }

    // ---- stator boundaries ----
    out.push(circle_arc(
        "stator.outer".into(),
        ro,
        c(SECTOR_LO),
        c(SECTOR_HI),
    ));
    for (nm, th) in [("lo", SECTOR_LO), ("hi", SECTOR_HI)] {
        out.push(line(
            format!("stator.edge_{nm}_iron"),
            polar(c(R_BORE), c(th)),
            polar(ro, c(th)),
        ));
        out.push(line(
            format!("stator.edge_{nm}_gap"),
            polar(c(R_COUPLING), c(th)),
            polar(c(R_BORE), c(th)),
        ));
    }
    out.push(circle_arc(
        "coupling".into(),
        c(R_COUPLING),
        c(SECTOR_LO),
        c(SECTOR_HI),
    ));
    Ok(out)
}

/// [`sector_curves`] in `f64`.
///
/// # Errors
///
/// As [`sector_curves`].
pub fn sector_curves_f64(design: &IcemDesign) -> Result<Vec<Curve<f64>>, IcemParamError> {
    sector_curves(&design.values)
}

/// [`sector_curves`] with the derivative of every coordinate with respect
/// to the 9 parameters.
///
/// # Errors
///
/// As [`sector_curves`].
pub fn sector_curves_dual(design: &IcemDesign) -> Result<Vec<Curve<Dual>>, IcemParamError> {
    let mut p = [Dual::cst(0.0); N_PARAMS];
    for (k, pk) in p.iter_mut().enumerate() {
        *pk = Dual::var(design.values[k], k);
    }
    sector_curves(&p)
}

// ── Node motion ─────────────────────────────────────────────────────────

/// How a node of the `θ0` mesh moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeAttachment {
    /// Interior to one material region: harmonic extension.
    Free,
    /// At the start (`end = 0`) or end (`end = 1`) point of a curve.
    Vertex {
        /// Curve index into [`sector_curves`].
        curve: usize,
        /// 0 = start, 1 = end.
        end: u8,
    },
    /// On a curve, strictly between its end points.
    OnCurve {
        /// Curve index.
        curve: usize,
        /// Parameter (length or angle fraction).
        t: f64,
    },
}

/// What [`IcemShapeMap::new`] found when it attached the mesh to the curves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttachmentReport {
    /// Nodes on a curve (vertices included).
    pub boundary_nodes: usize,
    /// Of those, nodes at a curve end point.
    pub vertex_nodes: usize,
    /// Interior (free) nodes.
    pub free_nodes: usize,
    /// Largest distance (m) between an attached node and its curve at `θ0`.
    pub max_attach_distance: f64,
    /// Nodes attached to each curve (vertices counted once, on the first
    /// curve that ends there), parallel to the curve list.
    pub nodes_per_curve: Vec<usize>,
}

/// How the stator's radial sector edges (`stator.edge_{lo,hi}_iron`, from
/// the bore to the outer circle) move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeRule {
    /// Nodes keep their fraction of the edge length (a prescribed radial
    /// stretch, like every other curve).
    Stretch,
    /// Each antiperiodic node pair (`x_hi = R(60°) x_lo`) shares one radial
    /// displacement, which the harmonic extension solves for. The pair stays
    /// rotation-related exactly; the edge does not impose a stretch on the
    /// iron next to it.
    Slide,
}

/// Options of the interior extension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionOptions {
    /// Radial sector-edge rule. Default [`EdgeRule::Slide`].
    pub edges: EdgeRule,
    /// Jacobian stiffening exponent `χ`: element `e` of the Laplacian is
    /// weighted by `A_e^{−χ}` (base-mesh area), so that small elements
    /// deform less. `0` is the plain harmonic extension. Default `1`.
    pub stiffening: f64,
}

impl Default for MotionOptions {
    fn default() -> Self {
        Self {
            edges: EdgeRule::Slide,
            stiffening: 1.0,
        }
    }
}

/// Role of a node in the extension system.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Dof {
    /// Not in any triangle.
    Unused,
    /// Prescribed by its curve; index into the boundary list.
    Dirichlet(usize),
    /// Interior; unknowns `2f` (x) and `2f + 1` (y).
    Free(usize),
    /// Sliding edge node: unknown `k` times direction `e`.
    Slide(usize, [f64; 2]),
}

/// The fixed-topology node motion `θ ↦ X(θ)` of a tagged ICEM sector mesh.
pub struct IcemShapeMap {
    theta0: IcemDesign,
    options: MotionOptions,
    base: TriMesh,
    tri_tags: Vec<i32>,
    curve_names: Vec<String>,
    attach: Vec<NodeAttachment>,
    /// Prescribed nodes and their position at `θ0` from the curves.
    boundary: Vec<usize>,
    xb0: Vec<[f64; 2]>,
    dof: Vec<Dof>,
    /// Right-hand-side couplings `(row, row direction, boundary slot, K_ij)`.
    rhs_terms: Vec<(usize, [f64; 2], usize, f64)>,
    lu: Option<Lu<usize, f64>>,
    n_red: usize,
    slide_pairs: usize,
    report: AttachmentReport,
}

/// Distance (m) within which a node counts as on a curve. Mesh nodes lie on
/// the exact CAD curves to round-off; the finest mesh spacing is 2.6e-5 m.
pub const ATTACH_TOL: f64 = 1e-9;

impl IcemShapeMap {
    /// Attach `tagged` (meshed at `theta0`) to the curves and factor the
    /// extension operator.
    ///
    /// # Errors
    ///
    /// [`IcemParamError::Classification`] if a node on a material interface
    /// or on the mesh boundary lies on no curve, if an unattached node
    /// touches two regions, or if a node lies on two curves away from their
    /// end points; [`IcemParamError::Construction`] /
    /// [`IcemParamError::Solve`] from the curves and the factorization.
    pub fn new(tagged: &TaggedTriMesh, theta0: IcemDesign) -> Result<Self, IcemParamError> {
        Self::with_options(tagged, theta0, MotionOptions::default())
    }

    /// [`IcemShapeMap::new`] with explicit [`MotionOptions`].
    ///
    /// # Errors
    ///
    /// As [`IcemShapeMap::new`].
    pub fn with_options(
        tagged: &TaggedTriMesh,
        theta0: IcemDesign,
        options: MotionOptions,
    ) -> Result<Self, IcemParamError> {
        let curves = sector_curves_f64(&theta0)?;
        let base = tagged.mesh.clone();
        let tri_tags = tagged.tri_physical_tags.clone();
        let n = base.n_nodes();

        // Nodes that must lie on a curve: ends of edges with one triangle
        // (domain boundary) or with triangles of two different regions.
        let mut edge_tags: std::collections::HashMap<(u32, u32), Vec<i32>> =
            std::collections::HashMap::new();
        for (t, tri) in base.tris.iter().enumerate() {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (tri[i].min(tri[j]), tri[i].max(tri[j]));
                edge_tags.entry((a, b)).or_default().push(tri_tags[t]);
            }
        }
        let mut must_attach = vec![false; n];
        for ((a, b), tags) in &edge_tags {
            if tags.len() == 1 || tags.iter().any(|&g| g != tags[0]) {
                must_attach[*a as usize] = true;
                must_attach[*b as usize] = true;
            }
        }
        let mut used = vec![false; n];
        for tri in &base.tris {
            for &v in tri {
                used[v as usize] = true;
            }
        }

        let mut attach = vec![NodeAttachment::Free; n];
        let mut report = AttachmentReport {
            nodes_per_curve: vec![0; curves.len()],
            ..AttachmentReport::default()
        };
        for (i, &p) in base.nodes.iter().enumerate() {
            if !used[i] {
                continue;
            }
            // Vertices first.
            let mut hit: Option<(NodeAttachment, f64)> = None;
            'v: for (ci, cv) in curves.iter().enumerate() {
                for (end, q) in [(0u8, cv.start()), (1u8, cv.end())] {
                    let d = (p[0] - q[0]).hypot(p[1] - q[1]);
                    if d <= ATTACH_TOL {
                        hit = Some((NodeAttachment::Vertex { curve: ci, end }, d));
                        break 'v;
                    }
                }
            }
            if hit.is_none() {
                let mut found: Vec<(usize, f64, f64)> = Vec::new();
                for (ci, cv) in curves.iter().enumerate() {
                    if let Some((d, t)) = cv.locate(p)
                        && d <= ATTACH_TOL
                    {
                        found.push((ci, t, d));
                    }
                }
                if found.len() > 1 {
                    return Err(IcemParamError::Classification(format!(
                        "node {i} at ({:.9e}, {:.9e}) lies on curves {:?} away from their ends",
                        p[0],
                        p[1],
                        found
                            .iter()
                            .map(|f| curves[f.0].name.as_str())
                            .collect::<Vec<_>>()
                    )));
                }
                if let Some(&(ci, t, d)) = found.first() {
                    hit = Some((NodeAttachment::OnCurve { curve: ci, t }, d));
                }
            }
            match hit {
                Some((a, d)) => {
                    attach[i] = a;
                    report.boundary_nodes += 1;
                    report.max_attach_distance = report.max_attach_distance.max(d);
                    match a {
                        NodeAttachment::Vertex { curve, .. } => {
                            report.vertex_nodes += 1;
                            report.nodes_per_curve[curve] += 1;
                        }
                        NodeAttachment::OnCurve { curve, .. } => {
                            report.nodes_per_curve[curve] += 1;
                        }
                        NodeAttachment::Free => {}
                    }
                }
                None if must_attach[i] => {
                    return Err(IcemParamError::Classification(format!(
                        "node {i} at ({:.9e}, {:.9e}) is on a material interface or the \
                         boundary but on no curve",
                        p[0], p[1]
                    )));
                }
                None => report.free_nodes += 1,
            }
        }

        // An unattached node must be interior to a single region.
        let mut node_tag: Vec<Option<i32>> = vec![None; n];
        for (t, tri) in base.tris.iter().enumerate() {
            for &v in tri {
                let v = v as usize;
                if attach[v] != NodeAttachment::Free {
                    continue;
                }
                match node_tag[v] {
                    None => node_tag[v] = Some(tri_tags[t]),
                    Some(g) if g != tri_tags[t] => {
                        return Err(IcemParamError::Classification(format!(
                            "free node {v} touches regions {g} and {}",
                            tri_tags[t]
                        )));
                    }
                    _ => {}
                }
            }
        }

        // Sliding sector-edge pairs (stator iron part of the radial edges).
        let idx = |name: &str| curves.iter().position(|c| c.name == name);
        let (lo_c, hi_c) = (
            idx("stator.edge_lo_iron").expect("curve"),
            idx("stator.edge_hi_iron").expect("curve"),
        );
        let mut dof = vec![Dof::Unused; n];
        let mut n_red = 0;
        let mut slide_pairs = 0;
        if options.edges == EdgeRule::Slide {
            let on = |c: usize| -> Vec<usize> {
                let mut v: Vec<usize> = (0..n)
                    .filter(|&i| {
                        used[i]
                            && matches!(attach[i], NodeAttachment::OnCurve { curve, .. } if curve == c)
                    })
                    .collect();
                v.sort_by(|&a, &b| {
                    let (pa, pb) = (base.nodes[a], base.nodes[b]);
                    pa[0].hypot(pa[1]).total_cmp(&pb[0].hypot(pb[1]))
                });
                v
            };
            let (lo, hi) = (on(lo_c), on(hi_c));
            if lo.len() != hi.len() {
                return Err(IcemParamError::Classification(format!(
                    "stator radial edges carry {} and {} interior nodes",
                    lo.len(),
                    hi.len()
                )));
            }
            let (e_lo, e_hi) = (
                [SECTOR_LO.cos(), SECTOR_LO.sin()],
                [SECTOR_HI.cos(), SECTOR_HI.sin()],
            );
            for (&a, &b) in lo.iter().zip(&hi) {
                let q = rot(base.nodes[a], SECTOR_HI - SECTOR_LO);
                let pb = base.nodes[b];
                if (q[0] - pb[0]).hypot(q[1] - pb[1]) > ATTACH_TOL {
                    return Err(IcemParamError::Classification(format!(
                        "stator edge nodes {a} and {b} are not antiperiodic images"
                    )));
                }
                dof[a] = Dof::Slide(n_red, e_lo);
                dof[b] = Dof::Slide(n_red, e_hi);
                n_red += 1;
                slide_pairs += 1;
            }
        }
        let mut boundary = Vec::new();
        for i in 0..n {
            if !used[i] || dof[i] != Dof::Unused {
                continue;
            }
            if attach[i] == NodeAttachment::Free {
                dof[i] = Dof::Free(n_red);
                n_red += 2;
            } else {
                dof[i] = Dof::Dirichlet(boundary.len());
                boundary.push(i);
            }
        }
        let xb0 = boundary
            .iter()
            .map(|&i| attached_point(&curves, attach[i]))
            .collect();

        // Weighted P1 Laplacian of the θ0 mesh, in the reduced unknowns.
        let rows = |d: Dof| -> Vec<(usize, [f64; 2])> {
            match d {
                Dof::Free(f) => vec![(f, [1.0, 0.0]), (f + 1, [0.0, 1.0])],
                Dof::Slide(k, e) => vec![(k, e)],
                Dof::Dirichlet(_) | Dof::Unused => Vec::new(),
            }
        };
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        let mut rhs_terms = Vec::new();
        for tri in &base.tris {
            let p = tri.map(|v| base.nodes[v as usize]);
            let area2 = ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]))
                .abs();
            let weight = if options.stiffening == 0.0 {
                1.0
            } else {
                (0.5 * area2).powf(-options.stiffening)
            };
            let g = [
                [p[1][1] - p[2][1], p[2][0] - p[1][0]],
                [p[2][1] - p[0][1], p[0][0] - p[2][0]],
                [p[0][1] - p[1][1], p[1][0] - p[0][0]],
            ];
            for a in 0..3 {
                let ra = rows(dof[tri[a] as usize]);
                if ra.is_empty() {
                    continue;
                }
                for b in 0..3 {
                    let kab = weight * (g[a][0] * g[b][0] + g[a][1] * g[b][1]) / (2.0 * area2);
                    let db = dof[tri[b] as usize];
                    if let Dof::Dirichlet(slot) = db {
                        for &(r, da) in &ra {
                            rhs_terms.push((r, da, slot, kab));
                        }
                        continue;
                    }
                    for &(r, da) in &ra {
                        for (c, dbv) in rows(db) {
                            let v = kab * (da[0] * dbv[0] + da[1] * dbv[1]);
                            if v != 0.0 {
                                trips.push(Triplet::new(r, c, v));
                            }
                        }
                    }
                }
            }
        }
        let lu = if n_red > 0 {
            let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_red, n_red, &trips)
                .map_err(|e| IcemParamError::Solve(format!("{e:?}")))?;
            Some(
                k.as_ref()
                    .sp_lu()
                    .map_err(|e| IcemParamError::Solve(format!("{e:?}")))?,
            )
        } else {
            None
        };
        Ok(Self {
            theta0,
            options,
            base,
            tri_tags,
            curve_names: curves.into_iter().map(|c| c.name).collect(),
            attach,
            boundary,
            xb0,
            dof,
            rhs_terms,
            lu,
            n_red,
            slide_pairs,
            report,
        })
    }

    /// The design the mesh was generated at.
    pub fn theta0(&self) -> IcemDesign {
        self.theta0
    }

    /// The extension options.
    pub fn options(&self) -> MotionOptions {
        self.options
    }

    /// Number of sliding antiperiodic node pairs on the stator radial edges.
    pub fn slide_pairs(&self) -> usize {
        self.slide_pairs
    }

    /// The `θ0` mesh (counter-clockwise triangles).
    pub fn base(&self) -> &TriMesh {
        &self.base
    }

    /// Per-triangle physical tags.
    pub fn tri_tags(&self) -> &[i32] {
        &self.tri_tags
    }

    /// Curve names, parallel to [`sector_curves`].
    pub fn curve_names(&self) -> &[String] {
        &self.curve_names
    }

    /// Per-node attachment.
    pub fn attachments(&self) -> &[NodeAttachment] {
        &self.attach
    }

    /// Attachment statistics.
    pub fn report(&self) -> &AttachmentReport {
        &self.report
    }

    /// Extend boundary displacement fields (one `[dx, dy]` per prescribed
    /// node, per field) to every node.
    fn extend(&self, fields: &[Vec<[f64; 2]>]) -> Vec<Vec<[f64; 2]>> {
        let m = fields.len();
        let mut rhs: Mat<f64> = Mat::zeros(self.n_red, m);
        for &(r, d, slot, k) in &self.rhs_terms {
            for (j, f) in fields.iter().enumerate() {
                rhs[(r, j)] -= k * (d[0] * f[slot][0] + d[1] * f[slot][1]);
            }
        }
        if let Some(lu) = &self.lu {
            lu.solve_in_place(rhs.as_mut());
        }
        let n = self.base.n_nodes();
        let mut out = vec![vec![[0.0; 2]; n]; m];
        for (i, d) in self.dof.iter().enumerate() {
            for (j, o) in out.iter_mut().enumerate() {
                o[i] = match *d {
                    Dof::Unused => [0.0, 0.0],
                    Dof::Dirichlet(slot) => fields[j][slot],
                    Dof::Free(f) => [rhs[(f, j)], rhs[(f + 1, j)]],
                    Dof::Slide(k, e) => [rhs[(k, j)] * e[0], rhs[(k, j)] * e[1]],
                };
            }
        }
        out
    }

    /// Node coordinates `X(θ)`.
    ///
    /// # Errors
    ///
    /// [`IcemParamError::Construction`] if `theta` defines no geometry.
    pub fn positions(&self, theta: &IcemDesign) -> Result<Vec<[f64; 2]>, IcemParamError> {
        let curves = sector_curves_f64(theta)?;
        let field: Vec<[f64; 2]> = self
            .boundary
            .iter()
            .zip(&self.xb0)
            .map(|(&i, x0)| {
                let q = attached_point(&curves, self.attach[i]);
                [q[0] - x0[0], q[1] - x0[1]]
            })
            .collect();
        let d = self.extend(&[field]).pop().expect("one field");
        Ok(self
            .base
            .nodes
            .iter()
            .zip(&d)
            .map(|(x, u)| [x[0] + u[0], x[1] + u[1]])
            .collect())
    }

    /// `∂X/∂θ_k` at `θ`: `out[k][node] = [∂x/∂θ_k, ∂y/∂θ_k]`, `k` in
    /// [`PARAM_NAMES`] order.
    ///
    /// # Errors
    ///
    /// [`IcemParamError::Construction`] if `theta` defines no geometry.
    pub fn jacobian(&self, theta: &IcemDesign) -> Result<Vec<Vec<[f64; 2]>>, IcemParamError> {
        let curves = sector_curves_dual(theta)?;
        let q: Vec<[Dual; 2]> = self
            .boundary
            .iter()
            .map(|&i| attached_point(&curves, self.attach[i]))
            .collect();
        let fields: Vec<Vec<[f64; 2]>> = (0..N_PARAMS)
            .map(|k| q.iter().map(|p| [p[0].d[k], p[1].d[k]]).collect())
            .collect();
        Ok(self.extend(&fields))
    }

    /// The `θ0` mesh with its nodes at `X(θ)`.
    ///
    /// # Errors
    ///
    /// As [`IcemShapeMap::positions`].
    pub fn moved_mesh(&self, theta: &IcemDesign) -> Result<TriMesh, IcemParamError> {
        Ok(TriMesh {
            nodes: self.positions(theta)?,
            tris: self.base.tris.clone(),
        })
    }
}

fn attached_point<T: Real>(curves: &[Curve<T>], a: NodeAttachment) -> [T; 2] {
    match a {
        NodeAttachment::Vertex { curve, end } => {
            if end == 0 {
                curves[curve].start()
            } else {
                curves[curve].end()
            }
        }
        NodeAttachment::OnCurve { curve, t } => curves[curve].point(t),
        NodeAttachment::Free => unreachable!("free nodes have no curve point"),
    }
}

// ── Mesh quality ────────────────────────────────────────────────────────

/// Element quality of a moved mesh against its base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionQuality {
    /// Smallest interior angle (degrees) over triangles with positive area.
    pub min_angle_deg: f64,
    /// Largest interior angle (degrees).
    pub max_angle_deg: f64,
    /// Smallest ratio of moved to base signed area (the P1 Jacobian ratio);
    /// `≤ 0` means an inverted or collapsed triangle.
    pub min_area_ratio: f64,
    /// Largest area ratio.
    pub max_area_ratio: f64,
    /// Triangles with non-positive moved area (base orientation).
    pub inverted: usize,
}

/// Quality of `nodes` on the triangles of `base` (counter-clockwise in the
/// base).
pub fn motion_quality(base: &TriMesh, nodes: &[[f64; 2]]) -> MotionQuality {
    let mut q = MotionQuality {
        min_angle_deg: f64::INFINITY,
        max_angle_deg: 0.0,
        min_area_ratio: f64::INFINITY,
        max_area_ratio: f64::NEG_INFINITY,
        inverted: 0,
    };
    let signed = |p: [[f64; 2]; 3]| {
        0.5 * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
            - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]))
    };
    for tri in &base.tris {
        let p0 = tri.map(|v| base.nodes[v as usize]);
        let p = tri.map(|v| nodes[v as usize]);
        let ratio = signed(p) / signed(p0);
        q.min_area_ratio = q.min_area_ratio.min(ratio);
        q.max_area_ratio = q.max_area_ratio.max(ratio);
        if ratio <= 0.0 {
            q.inverted += 1;
            continue;
        }
        for i in 0..3 {
            let a = p[i];
            let b = p[(i + 1) % 3];
            let c = p[(i + 2) % 3];
            let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
            let cosang = (u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1]));
            let ang = cosang.clamp(-1.0, 1.0).acos().to_degrees();
            q.min_angle_deg = q.min_angle_deg.min(ang);
            q.max_angle_deg = q.max_angle_deg.max(ang);
        }
    }
    q
}

/// The `k` triangles with the smallest moved-to-base area ratio:
/// `(triangle, area ratio, smallest angle in degrees)`.
pub fn worst_triangles(base: &TriMesh, nodes: &[[f64; 2]], k: usize) -> Vec<(usize, f64, f64)> {
    let signed = |p: [[f64; 2]; 3]| {
        0.5 * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
            - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]))
    };
    let mut all: Vec<(usize, f64, f64)> = base
        .tris
        .iter()
        .enumerate()
        .map(|(t, tri)| {
            let p = tri.map(|v| nodes[v as usize]);
            let ratio = signed(p) / signed(tri.map(|v| base.nodes[v as usize]));
            let mut amin = f64::INFINITY;
            for i in 0..3 {
                let (a, b, c) = (p[i], p[(i + 1) % 3], p[(i + 2) % 3]);
                let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
                let cs = (u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1]));
                amin = amin.min(cs.clamp(-1.0, 1.0).acos().to_degrees());
            }
            (t, ratio, amin)
        })
        .collect();
    all.sort_by(|a, b| a.1.total_cmp(&b.1));
    all.truncate(k);
    all
}

// ── MSH output ──────────────────────────────────────────────────────────

/// Rewrite the `$Nodes` coordinates of a Gmsh MSH 4.1 ASCII file.
///
/// `nodes[k]` replaces the `k`-th coordinate line in file order, which is
/// the node order of [`super::read_tagged_tri_mesh`]. Everything else
/// (entities, elements, physical names, triangle orientation as written) is
/// copied unchanged, so the result loads with the same tags. `z` is written
/// as 0. Coordinates use Rust's shortest round-trip format, so reading the
/// file back gives `nodes` bit for bit.
///
/// The reader normalises triangle orientation on load. A moved mesh with an
/// inverted triangle would therefore load as a valid mesh: check
/// [`motion_quality`] before writing.
///
/// # Errors
///
/// [`IcemParamError::Msh`] if the text has no `$Nodes` section, a block
/// carries parametric coordinates, or the node count differs from
/// `nodes.len()`.
pub fn rewrite_msh_nodes(msh: &str, nodes: &[[f64; 2]]) -> Result<String, IcemParamError> {
    let err = |m: String| IcemParamError::Msh(m);
    let mut out = String::with_capacity(msh.len() + msh.len() / 4);
    let mut lines = msh.split_inclusive('\n');
    let mut found = false;
    while let Some(l) = lines.next() {
        out.push_str(l);
        if l.trim_end() != "$Nodes" {
            continue;
        }
        found = true;
        let header = lines.next().ok_or_else(|| err("truncated $Nodes".into()))?;
        out.push_str(header);
        let h: Vec<usize> = header
            .split_whitespace()
            .map(|x| x.parse().map_err(|e| err(format!("$Nodes header: {e}"))))
            .collect::<Result<_, _>>()?;
        if h.len() != 4 {
            return Err(err(format!("$Nodes header has {} fields", h.len())));
        }
        let (n_blocks, n_nodes) = (h[0], h[1]);
        if n_nodes != nodes.len() {
            return Err(err(format!(
                "file has {n_nodes} nodes, {} coordinates given",
                nodes.len()
            )));
        }
        let mut k = 0usize;
        for _ in 0..n_blocks {
            let bh = lines
                .next()
                .ok_or_else(|| err("truncated node block".into()))?;
            out.push_str(bh);
            let f: Vec<usize> = bh
                .split_whitespace()
                .map(|x| {
                    x.parse()
                        .map_err(|e| err(format!("node block header: {e}")))
                })
                .collect::<Result<_, _>>()?;
            if f.len() != 4 || f[2] != 0 {
                return Err(err(format!(
                    "node block header {:?}: expected 4 fields, no parametric coordinates",
                    bh.trim_end()
                )));
            }
            let m = f[3];
            for _ in 0..m {
                out.push_str(
                    lines
                        .next()
                        .ok_or_else(|| err("truncated node tags".into()))?,
                );
            }
            for _ in 0..m {
                lines
                    .next()
                    .ok_or_else(|| err("truncated node coordinates".into()))?;
                let p = nodes[k];
                out.push_str(&format!("{} {} 0\n", p[0], p[1]));
                k += 1;
            }
        }
    }
    if !found {
        return Err(err("no $Nodes section".into()));
    }
    Ok(out)
}
