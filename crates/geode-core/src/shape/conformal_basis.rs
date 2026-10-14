//! **Mesh-independent smooth design bases for the curved conformal patch**
//! (conformal v5 E4, issue #1087).
//!
//! [`super::FreeformBoundaryMorph::harmonic_boundary`] gives one node-local
//! hat function per patch node. Such a design vector alternates sign from
//! node to node, cannot be carried to another mesh, and (because every DOF
//! moves a node radially) can never change the patch **outline**, which is
//! what sets the resonant length. This module replaces the hats by two
//! smooth, mesh-independent bases written in a surface chart of the bent
//! patch, both built with [`super::FreeformBoundaryMorph::from_columns`]:
//!
//! * **Radial (gap) basis.** A tensor-product B-spline `B_q(s, y)` over the
//!   patch rectangle, times a smooth cutoff at the pinned feed and a smooth
//!   decay over the substrate margin, moving every substrate-top node along
//!   the outward radial normal. Column `q` is the harmonic extension of
//!   `Σ_p B_q(s_p, y_p) · r̂_p` prescribed on the substrate-top nodes, which
//!   by linearity of the Laplace extension equals `Σ_p B_q(s_p, y_p) D_p`
//!   with `D_p` the node-local hat columns (a unit test checks this on the
//!   committed L0 fixture).
//! * **Outline (tangential) basis.** One 1-D B-spline per patch edge, moving
//!   the edge's nodes outward within the surface. The edge motion is
//!   extended into the patch interior by a smoothstep ramp that starts at
//!   the pinned feed, applied to **every substrate node in the column**
//!   (top, interior layers and ground alike) so the thin slab translates
//!   tangentially instead of shearing between a moving top and a pinned
//!   ground, and decays to zero over the substrate margin. A metal node
//!   sliding inside its own sheet does not change the conductor, so only
//!   the patch outline (and the feed inset) changes physically. The morph
//!   is linear in the coefficients, so a tangential step `d` along the
//!   base-point tangent leaves the cylinder by the chord error
//!   `d² / 2R` (0.18 mm at `d = 6` mm, `R = 100` mm); the whole slab
//!   column shares it, so the patch–ground gap is preserved and only the
//!   shell radius bulges locally.
//!
//! Both bases are functions of the chart coordinates `(s, y)` alone, so the
//! same coefficient vector describes the same surface on every mesh level
//! (the prescribed nodes of each level sample one continuous map). An
//! optional **Helmholtz filter** on the coefficient lattice
//! ([`helmholtz_lattice_filter`]) smooths the radial design further; it is
//! a fixed linear map, so the filtered columns are again an explicit
//! `from_columns` basis and the adjoint chain is unchanged.
//!
//! # Chart
//!
//! The bent fixtures wrap the flat slab about an axis parallel to `y`
//! through `(x = 0, z = z_axis)` ([`crate::mesh::patch::bend_axis_z`]):
//! a flat point `(x, y, z)` maps to radius `r = R + (z − h/2)` and angle
//! `φ = x / R`. The chart is `s = R φ` (the flat `x`, the arc length on the
//! substrate mid-plane) and `y`; the outward radial unit vector `r̂` and the
//! in-surface tangent `ŝ = ∂/∂φ / r` complete the frame
//! ([`CylinderChart`]).

use crate::assembly::electrostatic::ElectrostaticError;
use crate::driven::shape::pml_shell_nodes;
use crate::mesh::TetMesh;
use crate::mesh::patch::{PHYS_SUBSTRATE, PHYS_UPML, PatchFixture};

use super::harmonic_dirichlet_velocities;

/// `3t² − 2t³` on `[0, 1]`, clamped outside.
pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ─────────────────────────────────────────────────────────────────────────
// Chart of the bent patch.
// ─────────────────────────────────────────────────────────────────────────

/// Cylindrical chart of a fixture bent by [`PatchFixture::bent_conformal`]
/// (or the generator's identical node map): axis parallel to `y` through
/// `(x = 0, z = z_axis)`, bend radius `R`.
#[derive(Clone, Copy, Debug)]
pub struct CylinderChart {
    /// Bend radius `R` (mm): the substrate mid-plane radius.
    pub bend_radius: f64,
    /// `z` of the bend axis (mm), [`crate::mesh::patch::bend_axis_z`].
    pub z_axis: f64,
}

impl CylinderChart {
    /// Radius of a point about the bend axis.
    pub fn radius(&self, p: [f64; 3]) -> f64 {
        let dz = p[2] - self.z_axis;
        (p[0] * p[0] + dz * dz).sqrt()
    }

    /// Chart coordinates `(s, y)`: `s = R · atan2(x, z − z_axis)` is the
    /// mid-plane arc length (the flat-frame `x`), `y` is unchanged.
    pub fn s_y(&self, p: [f64; 3]) -> (f64, f64) {
        let phi = p[0].atan2(p[2] - self.z_axis);
        (self.bend_radius * phi, p[1])
    }

    /// Outward unit radial vector (away from the bend axis) at `p`.
    pub fn radial_dir(&self, p: [f64; 3]) -> [f64; 3] {
        let dz = p[2] - self.z_axis;
        let r = (p[0] * p[0] + dz * dz).sqrt().max(1e-300);
        [p[0] / r, 0.0, dz / r]
    }

    /// Unit in-surface tangent along `+s` at `p` (`∂/∂φ` normalized).
    pub fn tangent_s(&self, p: [f64; 3]) -> [f64; 3] {
        let dz = p[2] - self.z_axis;
        let r = (p[0] * p[0] + dz * dz).sqrt().max(1e-300);
        [dz / r, 0.0, -p[0] / r]
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 1-D clamped uniform B-splines (Cox–de Boor).
// ─────────────────────────────────────────────────────────────────────────

/// A clamped uniform B-spline basis of `n` functions on `[lo, hi]`.
///
/// The degree is `min(degree, n − 1)`: one function is the constant `1`,
/// two functions are the linear hats, and so on. The functions form a
/// partition of unity, so a constant coefficient vector is the uniform
/// field. Evaluation clamps `t` to `[lo, hi]`.
#[derive(Clone, Debug)]
pub struct BSpline1D {
    degree: usize,
    knots: Vec<f64>,
}

impl BSpline1D {
    /// `n ≥ 1` basis functions of degree `min(degree, n − 1)` on `[lo, hi]`
    /// (`hi > lo`), with `n − degree − 1` uniformly spaced interior knots.
    ///
    /// # Panics
    ///
    /// Panics if `n == 0` or `hi <= lo`.
    pub fn uniform_clamped(n: usize, degree: usize, lo: f64, hi: f64) -> Self {
        assert!(n >= 1, "BSpline1D: at least one basis function");
        assert!(hi > lo, "BSpline1D: empty interval [{lo}, {hi}]");
        let k = degree.min(n - 1);
        let n_interior = n - k - 1;
        let mut knots = Vec::with_capacity(n + k + 1);
        knots.extend(std::iter::repeat_n(lo, k + 1));
        for i in 1..=n_interior {
            knots.push(lo + (hi - lo) * i as f64 / (n_interior + 1) as f64);
        }
        knots.extend(std::iter::repeat_n(hi, k + 1));
        Self { degree: k, knots }
    }

    /// Number of basis functions.
    pub fn n(&self) -> usize {
        self.knots.len() - self.degree - 1
    }

    /// The (effective) polynomial degree.
    pub fn degree(&self) -> usize {
        self.degree
    }

    /// Interval `[lo, hi]`.
    pub fn range(&self) -> [f64; 2] {
        [self.knots[0], *self.knots.last().unwrap()]
    }

    /// All `n` basis values at `t` (clamped to the interval).
    pub fn eval(&self, t: f64) -> Vec<f64> {
        let n = self.n();
        let p = self.degree;
        let [lo, hi] = self.range();
        let u = t.clamp(lo, hi);
        // Knot span index i with knots[i] <= u < knots[i+1], i in [p, n-1].
        let mut i = n - 1;
        for k in p..n {
            if u < self.knots[k + 1] {
                i = k;
                break;
            }
        }
        // Cox–de Boor (The NURBS Book, A2.2).
        let mut nvals = vec![0.0_f64; p + 1];
        let mut left = vec![0.0_f64; p + 1];
        let mut right = vec![0.0_f64; p + 1];
        nvals[0] = 1.0;
        for j in 1..=p {
            left[j] = u - self.knots[i + 1 - j];
            right[j] = self.knots[i + j] - u;
            let mut saved = 0.0;
            for r in 0..j {
                let denom = right[r + 1] + left[j - r];
                let temp = if denom != 0.0 { nvals[r] / denom } else { 0.0 };
                nvals[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            nvals[j] = saved;
        }
        let mut out = vec![0.0_f64; n];
        for (r, v) in nvals.iter().enumerate() {
            out[i - p + r] = *v;
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Patch rectangle, feed cutoff, margin decay.
// ─────────────────────────────────────────────────────────────────────────

/// The patch footprint in the chart, `s ∈ [s_lo, s_hi]`, `y ∈ [y_lo, y_hi]`.
#[derive(Clone, Copy, Debug)]
pub struct PatchRect {
    /// `[s_lo, s_hi]` (mm).
    pub s: [f64; 2],
    /// `[y_lo, y_hi]` (mm).
    pub y: [f64; 2],
}

impl PatchRect {
    /// The closest point of the rectangle and the distance to it (0 inside).
    pub fn clamp_dist(&self, s: f64, y: f64) -> (f64, f64, f64) {
        let sc = s.clamp(self.s[0], self.s[1]);
        let yc = y.clamp(self.y[0], self.y[1]);
        let d = ((s - sc).powi(2) + (y - yc).powi(2)).sqrt();
        (sc, yc, d)
    }
}

/// Smooth cutoff about the pinned feed: `0` within `r_inner` of
/// `(s, y)`, `1` beyond `r_outer`, a smoothstep between.
#[derive(Clone, Copy, Debug)]
pub struct FeedCutoff {
    /// Feed centre in the chart (mm).
    pub s: f64,
    /// Feed centre in the chart (mm).
    pub y: f64,
    /// Radius inside which the cutoff is exactly 0 (covers the port nodes).
    pub r_inner: f64,
    /// Radius beyond which the cutoff is exactly 1.
    pub r_outer: f64,
}

impl FeedCutoff {
    /// The cutoff value at `(s, y)`.
    pub fn eval(&self, s: f64, y: f64) -> f64 {
        let d = ((s - self.s).powi(2) + (y - self.y).powi(2)).sqrt();
        smoothstep((d - self.r_inner) / (self.r_outer - self.r_inner).max(1e-12))
    }
}

/// `1` inside the rectangle, smoothstep to `0` at distance `decay` outside.
fn margin_decay(d: f64, decay: f64) -> f64 {
    if d <= 0.0 {
        1.0
    } else if decay <= 0.0 {
        0.0
    } else {
        1.0 - smoothstep(d / decay)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Radial (gap) basis.
// ─────────────────────────────────────────────────────────────────────────

/// Tensor-product B-spline radial basis over the patch, times the feed
/// cutoff and the margin decay. DOF `q = i_s · n_y + i_y`.
#[derive(Clone, Debug)]
pub struct RadialBasis {
    bs: BSpline1D,
    by: BSpline1D,
    rect: PatchRect,
    feed: Option<FeedCutoff>,
    decay: f64,
}

impl RadialBasis {
    /// `n_s × n_y` clamped uniform B-splines of the given degree over
    /// `rect`, with an optional feed cutoff and a margin decay length (mm)
    /// over which the field goes to zero outside the rectangle (`0` = hard
    /// zero outside).
    pub fn new(
        n_s: usize,
        n_y: usize,
        degree: usize,
        rect: PatchRect,
        feed: Option<FeedCutoff>,
        decay: f64,
    ) -> Self {
        Self {
            bs: BSpline1D::uniform_clamped(n_s, degree, rect.s[0], rect.s[1]),
            by: BSpline1D::uniform_clamped(n_y, degree, rect.y[0], rect.y[1]),
            rect,
            feed,
            decay,
        }
    }

    /// Number of DOFs `n_s · n_y`.
    pub fn n_dofs(&self) -> usize {
        self.bs.n() * self.by.n()
    }

    /// `(n_s, n_y)`.
    pub fn shape(&self) -> (usize, usize) {
        (self.bs.n(), self.by.n())
    }

    /// Control-lattice spacings `(Δs, Δy)`: the rectangle side over
    /// `max(n − 1, 1)`, the spacing of `n` coefficients placed uniformly
    /// over the side (not the B-spline knot spacing, which is
    /// `side / (n − degree)`). Used by the Helmholtz filter, which only
    /// needs a consistent length per lattice direction.
    pub fn lattice_spacing(&self) -> (f64, f64) {
        let (ns, ny) = self.shape();
        (
            (self.rect.s[1] - self.rect.s[0]) / (ns.max(2) - 1) as f64,
            (self.rect.y[1] - self.rect.y[0]) / (ny.max(2) - 1) as f64,
        )
    }

    /// All basis values at chart point `(s, y)`: `B_{i_s}(s_c) B_{i_y}(y_c)
    /// · cutoff(s, y) · decay(d)` with `(s_c, y_c, d)` the clamped point and
    /// the distance outside the rectangle.
    pub fn eval(&self, s: f64, y: f64) -> Vec<f64> {
        let (sc, yc, d) = self.rect.clamp_dist(s, y);
        let w = margin_decay(d, self.decay) * self.feed.map_or(1.0, |f| f.eval(s, y));
        let vs = self.bs.eval(sc);
        let vy = self.by.eval(yc);
        let mut out = Vec::with_capacity(vs.len() * vy.len());
        for a in &vs {
            for b in &vy {
                out.push(a * b * w);
            }
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Outline (tangential) basis.
// ─────────────────────────────────────────────────────────────────────────

/// One 1-D B-spline per patch edge, each DOF moving its edge **outward**
/// within the surface. DOF layout: `[+y edge (n), −y edge (n), +s edge (n),
/// −s edge (n)]`; the `±y` splines run along `s`, the `±s` splines along
/// `y`. The edge value is carried into the interior by a smoothstep ramp
/// from the pinned feed band to the edge, and decays over the margin
/// outside the rectangle. Being linear, the motion follows the base-point
/// tangent, not the arc: a step `d` lifts the whole slab column off the
/// cylinder by the chord error `d² / 2R` (see the module docs).
#[derive(Clone, Debug)]
pub struct OutlineBasis {
    along_s: BSpline1D,
    along_y: BSpline1D,
    rect: PatchRect,
    /// Feed centre `(s_f, y_f)` and half-width of the pinned feed band.
    feed_s: f64,
    feed_y: f64,
    feed_half: f64,
    decay: f64,
}

impl OutlineBasis {
    /// `n_per_edge` functions of the given degree per edge; ramps start
    /// `feed_half` away from the feed centre `(feed_s, feed_y)` (so the
    /// pinned port band carries zero motion) and reach 1 at the edge.
    pub fn new(
        n_per_edge: usize,
        degree: usize,
        rect: PatchRect,
        feed_s: f64,
        feed_y: f64,
        feed_half: f64,
        decay: f64,
    ) -> Self {
        Self {
            along_s: BSpline1D::uniform_clamped(n_per_edge, degree, rect.s[0], rect.s[1]),
            along_y: BSpline1D::uniform_clamped(n_per_edge, degree, rect.y[0], rect.y[1]),
            rect,
            feed_s,
            feed_y,
            feed_half,
            decay,
        }
    }

    /// Number of DOFs `4 · n_per_edge`.
    pub fn n_dofs(&self) -> usize {
        4 * self.along_s.n()
    }

    /// Functions per edge.
    pub fn n_per_edge(&self) -> usize {
        self.along_s.n()
    }

    fn ramp(from: f64, to: f64, v: f64) -> f64 {
        smoothstep((v - from) / (to - from))
    }

    /// In-surface displacement `(Δs, Δy)` per unit coefficient of every DOF
    /// at chart point `(s, y)`.
    pub fn eval(&self, s: f64, y: f64) -> Vec<[f64; 2]> {
        let (sc, yc, d) = self.rect.clamp_dist(s, y);
        let w = margin_decay(d, self.decay);
        let n = self.n_per_edge();
        let mut out = vec![[0.0_f64; 2]; 4 * n];
        // ±y edges: spline along s, ramp in y from the feed band to the edge.
        let r_yp = Self::ramp(self.feed_y + self.feed_half, self.rect.y[1], yc);
        let r_ym = Self::ramp(self.feed_y - self.feed_half, self.rect.y[0], yc);
        for (j, b) in self.along_s.eval(sc).iter().enumerate() {
            out[j] = [0.0, b * r_yp * w];
            out[n + j] = [0.0, -b * r_ym * w];
        }
        // ±s edges: spline along y, ramp in s from the feed band to the edge.
        let r_sp = Self::ramp(self.feed_s + self.feed_half, self.rect.s[1], sc);
        let r_sm = Self::ramp(self.feed_s - self.feed_half, self.rect.s[0], sc);
        for (j, b) in self.along_y.eval(yc).iter().enumerate() {
            out[2 * n + j] = [b * r_sp * w, 0.0];
            out[3 * n + j] = [-b * r_sm * w, 0.0];
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Helmholtz filter on the coefficient lattice.
// ─────────────────────────────────────────────────────────────────────────

/// The dense filter matrix `F = (I + r² L)⁻¹` of a Helmholtz design filter
/// on an `n_s × n_y` coefficient lattice with spacings `(Δs, Δy)`, where `L`
/// is the five-point lattice Laplacian with natural (Neumann) boundaries.
/// `F` is symmetric, preserves constants (`F·1 = 1`) and is the identity at
/// `radius = 0`. Index `q = i_s · n_y + i_y`; `F[i][j]` is row `i`.
///
/// Composing a basis with the filter replaces column `D_q` by
/// `Σ_j F[j][q] D_j` ([`filter_columns`]), so the design vector `x` acts
/// through the filtered coefficients `c = F x`.
pub fn helmholtz_lattice_filter(
    n_s: usize,
    n_y: usize,
    ds: f64,
    dy: f64,
    radius: f64,
) -> Vec<Vec<f64>> {
    let p = n_s * n_y;
    let idx = |i: usize, j: usize| i * n_y + j;
    // A = I + r² L.
    let mut a = vec![vec![0.0_f64; p]; p];
    for i in 0..n_s {
        for j in 0..n_y {
            let q = idx(i, j);
            a[q][q] += 1.0;
            let r2 = radius * radius;
            if i + 1 < n_s {
                let w = r2 / (ds * ds);
                let q2 = idx(i + 1, j);
                a[q][q] += w;
                a[q2][q2] += w;
                a[q][q2] -= w;
                a[q2][q] -= w;
            }
            if j + 1 < n_y {
                let w = r2 / (dy * dy);
                let q2 = idx(i, j + 1);
                a[q][q] += w;
                a[q2][q2] += w;
                a[q][q2] -= w;
                a[q2][q] -= w;
            }
        }
    }
    // Gauss–Jordan inverse (p is at most a few hundred).
    let mut f = vec![vec![0.0_f64; p]; p];
    for (i, row) in f.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for c in 0..p {
        let piv = (c..p)
            .max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())
            .unwrap();
        a.swap(c, piv);
        f.swap(c, piv);
        let d = a[c][c];
        for v in a[c].iter_mut() {
            *v /= d;
        }
        for v in f[c].iter_mut() {
            *v /= d;
        }
        for r in 0..p {
            if r == c {
                continue;
            }
            let m = a[r][c];
            if m == 0.0 {
                continue;
            }
            let (ac, fc) = (a[c].clone(), f[c].clone());
            for k in 0..p {
                a[r][k] -= m * ac[k];
                f[r][k] -= m * fc[k];
            }
        }
    }
    f
}

/// Compose morph columns with a dense coefficient map: the design vector
/// `x` acts through `c = F x`, so column `q` of the result is
/// `Σ_j F[j][q] D_j`.
///
/// # Panics
///
/// Panics if `f` is not `n × n` with `n = columns.len()`.
pub fn filter_columns(columns: &[Vec<[f64; 3]>], f: &[Vec<f64>]) -> Vec<Vec<[f64; 3]>> {
    let p = columns.len();
    assert_eq!(f.len(), p, "filter rows != columns");
    for row in f {
        assert_eq!(row.len(), p, "filter is not square");
    }
    let n = columns.first().map_or(0, |c| c.len());
    (0..p)
        .map(|q| {
            let mut col = vec![[0.0_f64; 3]; n];
            for (j, dj) in columns.iter().enumerate() {
                let w = f[j][q];
                if w == 0.0 {
                    continue;
                }
                for (acc, d) in col.iter_mut().zip(dj) {
                    acc[0] += w * d[0];
                    acc[1] += w * d[1];
                    acc[2] += w * d[2];
                }
            }
            col
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// Node roles of the bent patch fixture.
// ─────────────────────────────────────────────────────────────────────────

/// Node sets of a bent [`PatchFixture`] that the two bases prescribe or pin.
#[derive(Clone, Debug)]
pub struct ConformalPatchNodes {
    /// Nodes of the PEC patch triangles (sorted, unique).
    pub patch: Vec<u32>,
    /// Nodes of the substrate tets on the top (patch-side) surface,
    /// including the patch nodes (sorted, unique).
    pub substrate_top: Vec<u32>,
    /// Every node of a substrate tet (sorted, unique).
    pub slab: Vec<u32>,
    /// Slab nodes on the substrate's side faces: those at the slab's
    /// extreme `s` or `y` in the chart (sorted, unique). The margin decay
    /// of both bases reaches zero there, and the radial columns pin them so
    /// the substrate/air side interfaces never move.
    pub substrate_side: Vec<u32>,
    /// Nodes of the ground triangles.
    pub ground: Vec<u32>,
    /// Nodes of the lumped-port faces.
    pub port: Vec<u32>,
    /// Nodes of the outer-boundary triangles.
    pub outer: Vec<u32>,
    /// Nodes of any UPML tet ([`pml_shell_nodes`]).
    pub pml_shell: Vec<u32>,
    /// Mean radius of the patch nodes about the bend axis.
    pub r_top: f64,
}

fn sorted_unique(mut v: Vec<u32>) -> Vec<u32> {
    v.sort_unstable();
    v.dedup();
    v
}

fn tri_nodes(tris: &[[u32; 3]]) -> Vec<u32> {
    sorted_unique(tris.iter().flatten().copied().collect())
}

fn union(sets: &[&[u32]]) -> Vec<u32> {
    sorted_unique(sets.iter().flat_map(|s| s.iter().copied()).collect())
}

impl ConformalPatchNodes {
    /// Classify the nodes of `fixture` in `chart`. A substrate node is on the
    /// top surface when its radius is within `top_tol` (mm) of the patch
    /// nodes' mean radius.
    pub fn identify(fixture: &PatchFixture, chart: &CylinderChart, top_tol: f64) -> Self {
        let mesh = &fixture.mesh;
        let patch = tri_nodes(&fixture.patch_triangles());
        let r_top = patch
            .iter()
            .map(|&n| chart.radius(mesh.nodes[n as usize]))
            .sum::<f64>()
            / patch.len().max(1) as f64;
        let slab = sorted_unique(
            fixture
                .tets_with_tag(PHYS_SUBSTRATE)
                .iter()
                .flat_map(|&t| mesh.tets[t as usize].iter().copied())
                .collect(),
        );
        let substrate_top: Vec<u32> = slab
            .iter()
            .copied()
            .filter(|&n| (chart.radius(mesh.nodes[n as usize]) - r_top).abs() <= top_tol)
            .collect();
        let mut s_lim = [f64::INFINITY, f64::NEG_INFINITY];
        let mut y_lim = [f64::INFINITY, f64::NEG_INFINITY];
        for &n in &slab {
            let (s, y) = chart.s_y(mesh.nodes[n as usize]);
            s_lim = [s_lim[0].min(s), s_lim[1].max(s)];
            y_lim = [y_lim[0].min(y), y_lim[1].max(y)];
        }
        let substrate_side: Vec<u32> = slab
            .iter()
            .copied()
            .filter(|&n| {
                let (s, y) = chart.s_y(mesh.nodes[n as usize]);
                (s - s_lim[0]).abs() <= top_tol
                    || (s_lim[1] - s).abs() <= top_tol
                    || (y - y_lim[0]).abs() <= top_tol
                    || (y_lim[1] - y).abs() <= top_tol
            })
            .collect();
        let pml_tet_mask: Vec<bool> = fixture
            .tet_physical_tags
            .iter()
            .map(|&t| t == PHYS_UPML)
            .collect();
        let pml_shell: Vec<u32> = pml_shell_nodes(mesh, &pml_tet_mask)
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| p.then_some(i as u32))
            .collect();
        Self {
            patch,
            substrate_top,
            slab,
            substrate_side,
            ground: tri_nodes(&fixture.ground_triangles()),
            port: tri_nodes(&fixture.port().faces),
            outer: tri_nodes(&fixture.outer_boundary_triangles()),
            pml_shell,
            r_top,
        }
    }

    /// Nodes held at zero in the **radial** columns: the UPML shell, the
    /// port faces, the ground, the substrate side faces and the outer
    /// boundary.
    pub fn fixed_zero_radial(&self) -> Vec<u32> {
        union(&[
            &self.pml_shell,
            &self.port,
            &self.ground,
            &self.substrate_side,
            &self.outer,
        ])
    }

    /// Nodes held at zero in the **outline** columns: the UPML shell, the
    /// port faces and the outer boundary (the ground slides tangentially
    /// with the slab and is prescribed, not pinned).
    pub fn fixed_zero_outline(&self) -> Vec<u32> {
        union(&[&self.pml_shell, &self.port, &self.outer])
    }
}

/// Triangles of the substrate-top surface: faces of substrate tets whose
/// three nodes all lie on `substrate_top` (each once). Includes the patch
/// faces.
pub fn substrate_top_triangles(fixture: &PatchFixture, substrate_top: &[u32]) -> Vec<[u32; 3]> {
    let mut on_top = vec![false; fixture.mesh.n_nodes()];
    for &n in substrate_top {
        on_top[n as usize] = true;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for &t in &fixture.tets_with_tag(PHYS_SUBSTRATE) {
        let tet = fixture.mesh.tets[t as usize];
        for skip in 0..4 {
            let mut tri = [0u32; 3];
            let mut k = 0;
            for (i, &n) in tet.iter().enumerate() {
                if i != skip {
                    tri[k] = n;
                    k += 1;
                }
            }
            if tri.iter().all(|&n| on_top[n as usize]) {
                let mut key = tri;
                key.sort_unstable();
                if seen.insert(key) {
                    out.push(tri);
                }
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Column builders.
// ─────────────────────────────────────────────────────────────────────────

/// Harmonic extension of several prescribed node fields from one
/// factorization: `fields(node)` returns the per-column motion of each
/// prescribed node (all of length `n_cols`), zero on `fixed_zero` (which
/// wins over a prescription), harmonic elsewhere.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from
/// [`harmonic_dirichlet_velocities`].
pub fn harmonic_columns_from_fields(
    mesh: &TetMesh,
    prescribed: &[u32],
    n_cols: usize,
    fields: impl Fn(u32) -> Vec<[f64; 3]>,
    fixed_zero: &[u32],
) -> Result<Vec<Vec<[f64; 3]>>, ElectrostaticError> {
    let mut columns: Vec<Vec<(u32, [f64; 3])>> = vec![Vec::with_capacity(prescribed.len()); n_cols];
    for &n in prescribed {
        let v = fields(n);
        assert_eq!(v.len(), n_cols, "field at node {n} has {} entries", v.len());
        for (c, d) in v.into_iter().enumerate() {
            columns[c].push((n, d));
        }
    }
    harmonic_dirichlet_velocities(mesh, None, &columns, fixed_zero)
}

/// The **radial (gap) columns**: column `q` prescribes `scale · B_q(s, y) ·
/// r̂` on every substrate-top node, zero on
/// [`ConformalPatchNodes::fixed_zero_radial`], harmonic elsewhere
/// (substrate interior layers and the air). `scale` is the radial motion
/// (mm) per unit coefficient.
///
/// # Errors
///
/// Propagates the Laplace assembly / factorization errors.
pub fn radial_columns(
    mesh: &TetMesh,
    chart: &CylinderChart,
    basis: &RadialBasis,
    nodes: &ConformalPatchNodes,
    scale: f64,
) -> Result<Vec<Vec<[f64; 3]>>, ElectrostaticError> {
    let fixed = nodes.fixed_zero_radial();
    harmonic_columns_from_fields(
        mesh,
        &nodes.substrate_top,
        basis.n_dofs(),
        |n| {
            let p = mesh.nodes[n as usize];
            let (s, y) = chart.s_y(p);
            let r = chart.radial_dir(p);
            basis
                .eval(s, y)
                .into_iter()
                .map(|b| [b * scale * r[0], b * scale * r[1], b * scale * r[2]])
                .collect()
        },
        &fixed,
    )
}

/// The **outline (tangential) columns**: column `q` prescribes
/// `scale · (Δs_q ŝ + Δy_q ŷ)` on every substrate node (top, interior and
/// ground), zero on [`ConformalPatchNodes::fixed_zero_outline`], harmonic
/// in the air. `scale` is the tangential motion (mm) per unit coefficient.
///
/// # Errors
///
/// Propagates the Laplace assembly / factorization errors.
pub fn outline_columns(
    mesh: &TetMesh,
    chart: &CylinderChart,
    basis: &OutlineBasis,
    nodes: &ConformalPatchNodes,
    scale: f64,
) -> Result<Vec<Vec<[f64; 3]>>, ElectrostaticError> {
    let fixed = nodes.fixed_zero_outline();
    harmonic_columns_from_fields(
        mesh,
        &nodes.slab,
        basis.n_dofs(),
        |n| {
            let p = mesh.nodes[n as usize];
            let (s, y) = chart.s_y(p);
            let t = chart.tangent_s(p);
            basis
                .eval(s, y)
                .into_iter()
                .map(|[ds, dy]| {
                    [
                        scale * ds * t[0],
                        scale * (ds * t[1] + dy),
                        scale * ds * t[2],
                    ]
                })
                .collect()
        },
        &fixed,
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Copper-only bases on a fixed-thickness board (issue #1120).
// ─────────────────────────────────────────────────────────────────────────

/// Corner DOFs of the patch rectangle: four in-surface fields, one per
/// corner, each moving its corner **outward along the corner bisector** by
/// 1 mm per unit (a negative coefficient chamfers / rounds the corner),
/// with a smoothstep bump `1 − smoothstep(d / radius)` of the chart
/// distance `d` to the corner. DOF order: `(s_hi, y_hi)`, `(s_lo, y_hi)`,
/// `(s_hi, y_lo)`, `(s_lo, y_lo)`.
#[derive(Clone, Copy, Debug)]
pub struct CornerBasis {
    rect: PatchRect,
    radius: f64,
}

impl CornerBasis {
    /// Corner bumps of support `radius` (mm) on `rect`.
    ///
    /// # Panics
    ///
    /// Panics unless `radius > 0`.
    pub fn new(rect: PatchRect, radius: f64) -> Self {
        assert!(radius > 0.0, "CornerBasis: radius must be positive");
        Self { rect, radius }
    }

    /// Number of DOFs (4).
    pub fn n_dofs(&self) -> usize {
        4
    }

    /// In-surface displacement `(Δs, Δy)` per unit coefficient of each corner
    /// DOF at chart point `(s, y)`.
    pub fn eval(&self, s: f64, y: f64) -> Vec<[f64; 2]> {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let corners = [
            (self.rect.s[1], self.rect.y[1], h, h),
            (self.rect.s[0], self.rect.y[1], -h, h),
            (self.rect.s[1], self.rect.y[0], h, -h),
            (self.rect.s[0], self.rect.y[0], -h, -h),
        ];
        corners
            .iter()
            .map(|&(cs, cy, us, uy)| {
                let b = 1.0 - smoothstep((s - cs).hypot(y - cy) / self.radius);
                [us * b, uy * b]
            })
            .collect()
    }
}

/// Feed DOFs of the probe-fed patch: the coax probe's position along `y` (the
/// **inset depth** from the `+y` radiating edge) and its footprint **width**
/// (the lumped-port rectangle's extent along `y`). Both move nodes only along
/// `y`, the bend axis, so every node stays on its own cylinder.
///
/// * DOF 0, inset: `Δy = −g(d)`: one unit moves the probe 1 mm away from the
///   `+y` edge (deeper inset); `g = 1` within `r_inner` of the feed centre
///   (a rigid translation of the port and its surroundings), smoothstep to 0 at
///   `r_outer`.
/// * DOF 1, width: `Δy = ½ · clamp((y − y_f) / half_width, −1, 1) · g(d)`: one
///   unit widens the port rectangle by 1 mm (a uniform `y` scaling of the port
///   faces), the nodes beyond the port shifting rigidly by ±½ mm within
///   `r_inner`.
///
/// `r_inner` must cover the port (`> half_width`) and `r_outer` must stay
/// inside the patch edges so the outline does not move.
#[derive(Clone, Copy, Debug)]
pub struct FeedBasis {
    /// Feed centre in the chart (mm).
    pub s: f64,
    /// Feed centre in the chart (mm).
    pub y: f64,
    /// Half the port width along `y` (mm).
    pub half_width: f64,
    /// Radius of the rigid core (mm).
    pub r_inner: f64,
    /// Radius beyond which the fields vanish (mm).
    pub r_outer: f64,
}

impl FeedBasis {
    /// Number of DOFs (2: inset, width).
    pub fn n_dofs(&self) -> usize {
        2
    }

    /// In-surface displacement `(Δs, Δy)` per unit coefficient of `[inset,
    /// width]` at chart point `(s, y)`.
    pub fn eval(&self, s: f64, y: f64) -> Vec<[f64; 2]> {
        let d = (s - self.s).hypot(y - self.y);
        let g = 1.0 - smoothstep((d - self.r_inner) / (self.r_outer - self.r_inner).max(1e-12));
        let t = ((y - self.y) / self.half_width).clamp(-1.0, 1.0);
        vec![[0.0, -g], [0.0, 0.5 * t * g]]
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Exact on-surface tangential morph (issue #1120).
// ─────────────────────────────────────────────────────────────────────────

/// A **fixed-thickness** tangential morph of the bent patch: every substrate
/// node slides **exactly** within its own cylinder `r = const`, so the FR-4
/// stays a uniform `h`-thick shell and the patch stays on the `R + h/2`
/// offset surface for every design vector (issue #1120).
///
/// A design vector `x` gives each substrate node a chart displacement
/// `(Δs, Δy) = Σ_q x_q (a_q, b_q)` from in-surface fields (outline, corner,
/// feed) sampled at its base chart point; the node moves to angle
/// `φ₀ + Δs / R` (the whole radial column rotates together, so the slab is
/// not sheared) and `y₀ + Δy` at its base radius:
/// `P = (r₀ sin φ, y, z_axis + r₀ cos φ)`. This is nonlinear in `x`, unlike
/// [`outline_columns`], whose straight-line step leaves the cylinder by the
/// chord error `d² / 2R`. Air nodes follow the harmonic (Laplace) extension of
/// the first-order motion (`∂P/∂x` at `x = 0`), zero on the UPML shell and the
/// outer boundary; it is linear in `x` and only has to keep the air mesh
/// valid.
///
/// The design gradient chains the nodal gradient through the **exact**
/// Jacobian at `x`: `∂P/∂x_q = (r₀/R)(cos φ, 0, −sin φ) a_q + ŷ b_q` on the
/// substrate nodes and the harmonic columns elsewhere.
#[derive(Clone, Debug)]
pub struct ChartMorph {
    chart: CylinderChart,
    linear: super::FreeformBoundaryMorph,
    slab: Vec<u32>,
    is_slab: Vec<bool>,
    r0: Vec<f64>,
    phi0: Vec<f64>,
    y0: Vec<f64>,
    /// `cols[q][k]`: `(Δs, Δy)` per unit of DOF `q` at substrate node `slab[k]`.
    cols: Vec<Vec<[f64; 2]>>,
}

impl ChartMorph {
    /// Build the morph for `n_cols` in-surface fields `field(s, y)` (each
    /// returning `n_cols` `(Δs, Δy)` pairs per unit coefficient).
    ///
    /// # Errors
    ///
    /// Propagates the Laplace assembly / factorization errors of the
    /// harmonic air extension.
    pub fn build(
        mesh: &TetMesh,
        chart: &CylinderChart,
        nodes: &ConformalPatchNodes,
        n_cols: usize,
        field: impl Fn(f64, f64) -> Vec<[f64; 2]>,
    ) -> Result<Self, ElectrostaticError> {
        let r_mid = chart.bend_radius;
        let slab = nodes.slab.clone();
        let mut is_slab = vec![false; mesh.n_nodes()];
        let mut r0 = Vec::with_capacity(slab.len());
        let mut phi0 = Vec::with_capacity(slab.len());
        let mut y0 = Vec::with_capacity(slab.len());
        let mut cols = vec![Vec::with_capacity(slab.len()); n_cols];
        for &n in &slab {
            is_slab[n as usize] = true;
            let p = mesh.nodes[n as usize];
            let (s, y) = chart.s_y(p);
            r0.push(chart.radius(p));
            phi0.push(s / r_mid);
            y0.push(y);
            let v = field(s, y);
            assert_eq!(v.len(), n_cols, "field at node {n} has {} entries", v.len());
            for (c, d) in v.into_iter().enumerate() {
                cols[c].push(d);
            }
        }
        // First-order (x = 0) Cartesian motion of the substrate nodes, extended
        // harmonically into the air; zero on the UPML shell and the outer wall.
        let fixed = union(&[&nodes.pml_shell, &nodes.outer]);
        let index: std::collections::HashMap<u32, usize> =
            slab.iter().enumerate().map(|(k, &n)| (n, k)).collect();
        let linear_cols = harmonic_columns_from_fields(
            mesh,
            &slab,
            n_cols,
            |n| {
                let k = index[&n];
                let p = mesh.nodes[n as usize];
                let t = chart.tangent_s(p);
                let scale = r0[k] / r_mid;
                (0..n_cols)
                    .map(|q| {
                        let [ds, dy] = cols[q][k];
                        [scale * ds * t[0], dy, scale * ds * t[2]]
                    })
                    .collect()
            },
            &fixed,
        )?;
        let linear = super::FreeformBoundaryMorph::from_columns(linear_cols)?;
        Ok(Self {
            chart: *chart,
            linear,
            slab,
            is_slab,
            r0,
            phi0,
            y0,
            cols,
        })
    }

    /// Number of design DOFs.
    pub fn n_dofs(&self) -> usize {
        self.cols.len()
    }

    /// The substrate nodes the morph slides exactly on their cylinders.
    pub fn slab_nodes(&self) -> &[u32] {
        &self.slab
    }

    /// Chart displacement `(Δs, Δy)` of substrate node `slab_nodes()[k]`.
    fn chart_disp(&self, k: usize, x: &[f64]) -> (f64, f64) {
        let (mut ds, mut dy) = (0.0, 0.0);
        for (xq, col) in x.iter().zip(&self.cols) {
            if *xq != 0.0 {
                ds += xq * col[k][0];
                dy += xq * col[k][1];
            }
        }
        (ds, dy)
    }

    /// The morphed mesh at `x` (topology fixed; `x = 0` reproduces `mesh`).
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != n_dofs()` or the mesh does not match the build.
    pub fn apply(&self, mesh: &TetMesh, x: &[f64]) -> TetMesh {
        assert_eq!(x.len(), self.n_dofs(), "ChartMorph: design length");
        let mut moved = self.linear.apply(mesh, x);
        let r_mid = self.chart.bend_radius;
        for (k, &n) in self.slab.iter().enumerate() {
            let (ds, dy) = self.chart_disp(k, x);
            if ds == 0.0 && dy == 0.0 {
                // Bit-exact base position (avoids the round trip through φ).
                moved.nodes[n as usize] = mesh.nodes[n as usize];
                continue;
            }
            let phi = self.phi0[k] + ds / r_mid;
            moved.nodes[n as usize] = [
                self.r0[k] * phi.sin(),
                self.y0[k] + dy,
                self.chart.z_axis + self.r0[k] * phi.cos(),
            ];
        }
        moved
    }

    /// Nodal displacement `X(x) − X⁰` (length `n_nodes`).
    pub fn displacement(&self, mesh: &TetMesh, x: &[f64]) -> Vec<[f64; 3]> {
        let moved = self.apply(mesh, x);
        moved
            .nodes
            .iter()
            .zip(&mesh.nodes)
            .map(|(a, b)| [a[0] - b[0], a[1] - b[1], a[2] - b[2]])
            .collect()
    }

    /// Chart displacement `(Δs, Δy)` of every substrate node at `x`, parallel
    /// to [`Self::slab_nodes`].
    pub fn chart_displacements(&self, x: &[f64]) -> Vec<[f64; 2]> {
        (0..self.slab.len())
            .map(|k| {
                let (ds, dy) = self.chart_disp(k, x);
                [ds, dy]
            })
            .collect()
    }

    /// Design gradient `∂g/∂x_q = Σ_n grad_node_n · ∂X_n/∂x_q (x)` through the
    /// exact Jacobian at `x` (substrate nodes) and the harmonic columns (air).
    ///
    /// # Panics
    ///
    /// Panics if the lengths do not match the build.
    pub fn design_gradient(&self, x: &[f64], grad_node: &[[f64; 3]]) -> Vec<f64> {
        assert_eq!(x.len(), self.n_dofs(), "ChartMorph: design length");
        let mut air = grad_node.to_vec();
        for &n in &self.slab {
            air[n as usize] = [0.0; 3];
        }
        let mut g = self.linear.design_gradient(&air);
        let r_mid = self.chart.bend_radius;
        for (k, &n) in self.slab.iter().enumerate() {
            let gn = grad_node[n as usize];
            if gn == [0.0; 3] {
                continue;
            }
            let (ds, _) = self.chart_disp(k, x);
            let phi = self.phi0[k] + ds / r_mid;
            let sc = self.r0[k] / r_mid;
            // ∂P/∂(Δs) and ∂P/∂(Δy).
            let dps = [sc * phi.cos(), 0.0, -sc * phi.sin()];
            let a = gn[0] * dps[0] + gn[2] * dps[2];
            let b = gn[1];
            for (gq, col) in g.iter_mut().zip(&self.cols) {
                let [cs, cy] = col[k];
                *gq += a * cs + b * cy;
            }
        }
        g
    }

    /// `true` for the substrate nodes (length `n_nodes`).
    pub fn is_slab(&self) -> &[bool] {
        &self.is_slab
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bspline_partition_of_unity_and_limits() {
        for (n, deg) in [(1, 3), (2, 3), (3, 3), (4, 3), (8, 3), (6, 2), (5, 1)] {
            let b = BSpline1D::uniform_clamped(n, deg, -2.0, 3.0);
            assert_eq!(b.n(), n);
            assert_eq!(b.degree(), deg.min(n - 1));
            for k in 0..=50 {
                let t = -2.0 + 5.0 * k as f64 / 50.0;
                let v = b.eval(t);
                let sum: f64 = v.iter().sum();
                assert!((sum - 1.0).abs() < 1e-12, "n={n} deg={deg} t={t}: {sum}");
                assert!(v.iter().all(|x| *x >= -1e-15));
            }
            // Clamped ends: the first / last function is 1 at the ends.
            let v0 = b.eval(-2.0);
            let v1 = b.eval(3.0);
            assert!((v0[0] - 1.0).abs() < 1e-12);
            assert!((v1[n - 1] - 1.0).abs() < 1e-12);
            // Clamping outside the interval.
            assert_eq!(b.eval(-5.0), v0);
            assert_eq!(b.eval(9.0), v1);
        }
        // Two functions are the linear hats.
        let b = BSpline1D::uniform_clamped(2, 3, 0.0, 1.0);
        let v = b.eval(0.25);
        assert!((v[0] - 0.75).abs() < 1e-12 && (v[1] - 0.25).abs() < 1e-12);
    }

    #[test]
    fn helmholtz_filter_preserves_constants_and_is_identity_at_zero_radius() {
        let (ns, ny) = (4, 3);
        let f0 = helmholtz_lattice_filter(ns, ny, 1.5, 2.0, 0.0);
        for (i, row) in f0.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1e-12);
            }
        }
        let f = helmholtz_lattice_filter(ns, ny, 1.5, 2.0, 2.5);
        for (i, row) in f.iter().enumerate() {
            let sum: f64 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-10, "row {i} sums to {sum}");
            for (j, v) in row.iter().enumerate() {
                assert!((v - f[j][i]).abs() < 1e-10, "asymmetric at ({i},{j})");
                assert!(*v > -1e-12, "negative weight at ({i},{j}): {v}");
            }
            // The filter spreads mass off the diagonal.
            assert!(row[i] < 1.0);
        }
        // filter_columns: constants pass through, a single spike spreads.
        let n_nodes = 5;
        let cols: Vec<Vec<[f64; 3]>> = (0..ns * ny)
            .map(|q| vec![[q as f64, 1.0, 0.0]; n_nodes])
            .collect();
        let fc = filter_columns(&cols, &f);
        for (q, c) in fc.iter().enumerate() {
            let want_x: f64 = (0..ns * ny).map(|j| f[j][q] * j as f64).sum();
            assert!((c[0][0] - want_x).abs() < 1e-10);
            assert!((c[0][1] - 1.0).abs() < 1e-10, "column {q}: {}", c[0][1]);
        }
    }

    #[test]
    fn radial_basis_is_a_partition_of_unity_with_cutoff_and_decay() {
        let rect = PatchRect {
            s: [-19.0, 19.0],
            y: [-14.5, 14.5],
        };
        let feed = FeedCutoff {
            s: 0.0,
            y: 7.5,
            r_inner: 1.2,
            r_outer: 4.0,
        };
        let basis = RadialBasis::new(8, 6, 3, rect, Some(feed), 12.0);
        assert_eq!(basis.n_dofs(), 48);
        // Away from the feed and inside: sums to 1.
        let v = basis.eval(-10.0, -5.0);
        assert!((v.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // At the feed: zero.
        assert!(basis.eval(0.3, 7.9).iter().all(|x| *x == 0.0));
        // Outside by half the margin: between 0 and 1; at the margin: 0.
        let half: f64 = basis.eval(19.0 + 6.0, 0.0).iter().sum();
        assert!(half > 0.0 && half < 1.0);
        assert!(basis.eval(19.0 + 12.0, 0.0).iter().all(|x| *x == 0.0));
        assert!(basis.eval(0.0, -14.5 - 12.0).iter().all(|x| *x == 0.0));
        // P = 1 is the uniform field.
        let one = RadialBasis::new(1, 1, 3, rect, None, 0.0);
        assert_eq!(one.eval(3.0, -2.0), vec![1.0]);
        assert_eq!(one.eval(30.0, -2.0), vec![0.0]);
    }

    #[test]
    fn outline_basis_vanishes_on_the_feed_band_and_grows_outward() {
        let rect = PatchRect {
            s: [-19.0, 19.0],
            y: [-14.5, 14.5],
        };
        let basis = OutlineBasis::new(4, 3, rect, 0.0, 7.5, 0.8, 12.0);
        assert_eq!(basis.n_dofs(), 16);
        // Port band: nothing moves.
        for (s, y) in [(0.0, 7.5), (0.5, 8.2), (-0.7, 6.8)] {
            assert!(
                basis.eval(s, y).iter().all(|d| d == &[0.0, 0.0]),
                "({s},{y})"
            );
        }
        // On the +y edge the ramp is 1 and the splines sum to 1 in Δy.
        let v = basis.eval(5.0, 14.5);
        let dy_plus: f64 = v[0..4].iter().map(|d| d[1]).sum();
        assert!((dy_plus - 1.0).abs() < 1e-12);
        assert!(v[4..8].iter().all(|d| d[1] == 0.0));
        // On the −y edge the motion is outward (−y).
        let v = basis.eval(5.0, -14.5);
        let dy_minus: f64 = v[4..8].iter().map(|d| d[1]).sum();
        assert!((dy_minus + 1.0).abs() < 1e-12);
        // ±s edges move along ±s.
        let v = basis.eval(19.0, 0.0);
        assert!((v[8..12].iter().map(|d| d[0]).sum::<f64>() - 1.0).abs() < 1e-12);
        let v = basis.eval(-19.0, 0.0);
        assert!((v[12..16].iter().map(|d| d[0]).sum::<f64>() + 1.0).abs() < 1e-12);
        // Interior ramp is monotone between the feed band and the edge.
        let mut last = 0.0;
        for k in 0..=20 {
            let y = 8.3 + (14.5 - 8.3) * k as f64 / 20.0;
            let dy: f64 = basis.eval(-3.0, y)[0..4].iter().map(|d| d[1]).sum();
            assert!(dy >= last - 1e-12);
            last = dy;
        }
        // Beyond the margin: zero.
        assert!(
            basis
                .eval(0.0, 14.5 + 12.0)
                .iter()
                .all(|d| d == &[0.0, 0.0])
        );
    }

    #[test]
    fn chart_frame_is_orthonormal_and_inverts_the_bend() {
        let chart = CylinderChart {
            bend_radius: 100.0,
            z_axis: 0.8 - 100.0,
        };
        for (x_flat, y, z_flat) in [
            (-19.0_f64, 3.0_f64, 1.6_f64),
            (7.5, -14.5, 0.0),
            (31.0, 1.0, 0.8),
            (0.0, 2.0, 1.2),
        ] {
            let r: f64 = 100.0 + (z_flat - 0.8);
            let phi: f64 = x_flat / 100.0;
            let p = [r * phi.sin(), y, chart.z_axis + r * phi.cos()];
            let (s, yy) = chart.s_y(p);
            assert!((s - x_flat).abs() < 1e-9 && (yy - y).abs() < 1e-12);
            assert!((chart.radius(p) - r).abs() < 1e-9);
            let rd = chart.radial_dir(p);
            let ts = chart.tangent_s(p);
            let dot = rd[0] * ts[0] + rd[1] * ts[1] + rd[2] * ts[2];
            assert!(dot.abs() < 1e-12);
            assert!((rd[0] * rd[0] + rd[2] * rd[2] - 1.0).abs() < 1e-12);
            assert!((ts[0] * ts[0] + ts[2] * ts[2] - 1.0).abs() < 1e-12);
            // +s tangent points along increasing flat x at every phi:
            // ∂p/∂x_flat = (r / R)(cos phi, 0, -sin phi), so ŝ · ∂p/∂x_flat
            // must equal r / R > 0.
            let dp = [r / 100.0 * phi.cos(), 0.0, -r / 100.0 * phi.sin()];
            let along = ts[0] * dp[0] + ts[1] * dp[1] + ts[2] * dp[2];
            assert!(
                (along - r / 100.0).abs() < 1e-12,
                "tangent_s sign at phi = {phi}"
            );
            // At phi = 0 (the (0.0, 2.0, 1.2) sample) it is +x.
            if phi.abs() < 1e-12 {
                assert!(ts[0] > 0.0);
            }
        }
    }

    #[test]
    fn corner_and_feed_bases_move_what_they_claim() {
        let rect = PatchRect {
            s: [-19.0, 19.0],
            y: [-14.5, 14.5],
        };
        let cb = CornerBasis::new(rect, 6.0);
        assert_eq!(cb.n_dofs(), 4);
        let h = std::f64::consts::FRAC_1_SQRT_2;
        // Each corner moves outward along its bisector by 1 per unit; the others
        // (38 / 29 mm away) stay put.
        let corners = [
            (19.0, 14.5, h, h),
            (-19.0, 14.5, -h, h),
            (19.0, -14.5, h, -h),
            (-19.0, -14.5, -h, -h),
        ];
        for (q, &(s, y, us, uy)) in corners.iter().enumerate() {
            let v = cb.eval(s, y);
            for (j, d) in v.iter().enumerate() {
                if j == q {
                    assert!((d[0] - us).abs() < 1e-12 && (d[1] - uy).abs() < 1e-12);
                } else {
                    assert_eq!(d, &[0.0, 0.0]);
                }
            }
        }
        // Compact support: nothing at 6 mm from every corner, monotone decay.
        assert!(cb.eval(13.0, 14.5).iter().all(|d| d == &[0.0, 0.0]));
        let a = cb.eval(17.0, 14.5)[0][0];
        let b = cb.eval(15.0, 14.5)[0][0];
        assert!(a > b && b > 0.0);

        let fb = FeedBasis {
            s: 0.0,
            y: 7.5,
            half_width: 0.8,
            r_inner: 1.2,
            r_outer: 6.4,
        };
        assert_eq!(fb.n_dofs(), 2);
        // Only y motion, everywhere.
        for (s, y) in [(0.0, 7.5), (0.5, 8.3), (3.0, 4.0), (-2.0, 9.0)] {
            assert!(fb.eval(s, y).iter().all(|d| d[0] == 0.0));
        }
        // Inset: a rigid -1 mm y shift over the whole port (|y - y_f| <= 0.8).
        for y in [6.7, 7.5, 8.3] {
            assert_eq!(fb.eval(0.0, y)[0], [0.0, -1.0]);
        }
        // Width: the port edges move by -/+ 0.5 (width +1 per unit), linearly
        // in between, and the centre does not move.
        assert!((fb.eval(0.0, 8.3)[1][1] - 0.5).abs() < 1e-12);
        assert!((fb.eval(0.0, 6.7)[1][1] + 0.5).abs() < 1e-12);
        assert_eq!(fb.eval(0.0, 7.5)[1][1], 0.0);
        // Both vanish at r_outer, short of the +y edge (7 mm from the feed).
        assert!(fb.eval(0.0, 7.5 + 6.4).iter().all(|d| d == &[0.0, 0.0]));
        assert!(fb.eval(0.0, 14.5).iter().all(|d| d == &[0.0, 0.0]));
    }
}
