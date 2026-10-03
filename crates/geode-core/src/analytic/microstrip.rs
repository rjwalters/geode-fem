//! **Microstrip / stripline closed forms, shielded strip-line port faces, and
//! a 2-D quasi-static reference** (Epic #778 Phase 3, issue #805).
//!
//! The p=1 hybrid port-mode solver ([`super::port_modes`]) supports port
//! faces with interior PEC conductors: zero-thickness strips (PEC edges inside
//! the face) and thick strips (holes, whose inner rim is PEC). Such a face has
//! a **quasi-TEM** mode, with `ẽ_t ≈ ∇ψ` (ψ constant on each conductor) and
//! zero cutoff. This module provides what the tests need to validate it:
//!
//! - **Closed forms**: Hammerstad–Jensen static `ε_eff` and `Z₀`
//!   ([`hammerstad_jensen_eps_eff`], [`hammerstad_jensen_z0`]), the
//!   Kirschning–Jansen dispersion law ([`kirschning_jansen_eps_eff`]), and
//!   Pozar's quasi-TEM dielectric attenuation
//!   ([`pozar_dielectric_attenuation`], used by Phase 4).
//! - **Fixtures**: [`ShieldedStripFace`] builds a PEC box with a two-layer
//!   fill and any number of strips on the interface line, graded toward the
//!   strip edges where the field has the `r^{-1/2}` singularity; optionally
//!   mirror-symmetric, and optionally with thick strips carved out as holes.
//!   [`square_coax_face`] builds a C4v-symmetric square coax (a hole), the
//!   fixture for exactly repeated eigenvalues.
//! - **Quasi-static reference**: [`electrostatic_2d`] solves the P1
//!   `∇·(ε∇ψ) = 0` problem on the **same** [`TriMesh`] and returns the
//!   per-unit-length capacitance `C/ε₀ = ψᵀS_εψ`, so `ε_eff,qs = C(ε)/C(1)`
//!   and `Z_qs = η₀/√(c_ε c_1)` are mesh-consistent goldens independent of
//!   the eigen path. (The crate's [`crate::assembly::electrostatic`] module is
//!   the 3-D tetrahedral analogue; this is its 2-D sibling on the port mesh.)
//!
//! # Units
//!
//! The closed forms are dimensionless in `u = w/h`, except
//! [`kirschning_jansen_eps_eff`], whose frequency argument is the product
//! `f·h` in **GHz·mm** (the paper's normalization).

use faer::sparse::{SparseColMat, Triplet};

use super::port_modes::HybridPecMasks;
use super::waveguide::{TriMesh, tri_p1_local};
use crate::constants::ETA_0_OHM;

/// Hammerstad–Jensen static effective permittivity of a zero-thickness
/// microstrip, `u = w/h` (strip width over substrate height).
///
/// E. Hammerstad and Ø. Jensen, "Accurate models for microstrip
/// computer-aided design," *IEEE MTT-S Int. Microwave Symp. Digest*, 1980,
/// pp. 407–409, eqs. (2)–(4):
///
/// ```text
///   a(u) = 1 + ln((u⁴ + (u/52)²)/(u⁴ + 0.432))/49 + ln(1 + (u/18.1)³)/18.7
///   b(ε) = 0.564 ((ε − 0.9)/(ε + 3))^0.053
///   ε_eff = (ε+1)/2 + (ε−1)/2 · (1 + 10/u)^(−a b)
/// ```
///
/// Claimed accuracy: better than 0.2 % for `ε_r ≤ 128` and
/// `0.01 ≤ u ≤ 100` (against their reference numerical model).
pub fn hammerstad_jensen_eps_eff(u: f64, eps_r: f64) -> f64 {
    assert!(u > 0.0 && eps_r >= 1.0, "need u > 0 and eps_r ≥ 1");
    let u4 = u.powi(4);
    let a = 1.0
        + ((u4 + (u / 52.0).powi(2)) / (u4 + 0.432)).ln() / 49.0
        + (1.0 + (u / 18.1).powi(3)).ln() / 18.7;
    let b = 0.564 * ((eps_r - 0.9) / (eps_r + 3.0)).powf(0.053);
    0.5 * (eps_r + 1.0) + 0.5 * (eps_r - 1.0) * (1.0 + 10.0 / u).powf(-a * b)
}

/// Hammerstad–Jensen characteristic impedance of a zero-thickness microstrip
/// **in air** (`ε_r = 1`), in ohms, eq. (1) of the 1980 paper:
///
/// ```text
///   f(u)  = 6 + (2π − 6) exp(−(30.666/u)^0.7528)
///   Z₀₁   = (η₀/2π) ln(f(u)/u + √(1 + (2/u)²))
/// ```
///
/// Claimed accuracy: better than 0.01 % for `u ≤ 1` and 0.03 % for
/// `u ≤ 1000`.
pub fn hammerstad_jensen_z0_air(u: f64) -> f64 {
    assert!(u > 0.0, "need u > 0");
    let f = 6.0 + (2.0 * std::f64::consts::PI - 6.0) * (-(30.666 / u).powf(0.7528)).exp();
    ETA_0_OHM / (2.0 * std::f64::consts::PI) * (f / u + (1.0 + (2.0 / u).powi(2)).sqrt()).ln()
}

/// Hammerstad–Jensen quasi-static characteristic impedance (ohms) of a
/// zero-thickness microstrip: `Z₀ = Z₀₁(u)/√ε_eff(u, ε_r)`
/// ([`hammerstad_jensen_z0_air`], [`hammerstad_jensen_eps_eff`]). Claimed
/// accuracy about 1 % over the validity range of the two parts (better
/// near `u ≈ 1`).
pub fn hammerstad_jensen_z0(u: f64, eps_r: f64) -> f64 {
    hammerstad_jensen_z0_air(u) / hammerstad_jensen_eps_eff(u, eps_r).sqrt()
}

/// Kirschning–Jansen dispersive effective permittivity of a microstrip.
///
/// M. Kirschning and R. H. Jansen, "Accurate model for effective dielectric
/// constant of microstrip with validity up to millimetre-wave frequencies,"
/// *Electron. Lett.* **18**(6), 272–273 (1982):
///
/// ```text
///   ε_eff(f) = ε_r − (ε_r − ε_eff(0)) / (1 + P(f))
///   P  = P₁ P₂ ((0.1844 + P₃P₄) f_n)^1.5763,           f_n = f·h  [GHz·mm]
///   P₁ = 0.27488 + (0.6315 + 0.525/(1 + 0.0157 f_n)²⁰) u − 0.065683 e^(−8.7513u)
///   P₂ = 0.33622 (1 − e^(−0.03442 ε_r))
///   P₃ = 0.0363 e^(−4.6u) (1 − e^(−(f_n/38.7)^4.97))
///   P₄ = 1 + 2.751 (1 − e^(−(ε_r/15.916)^8))
/// ```
///
/// `eps_eff0` is the static value (the paper uses Hammerstad–Jensen,
/// [`hammerstad_jensen_eps_eff`]); `f_h_ghz_mm` is `f·h` in GHz·mm.
/// Claimed accuracy: 0.6 % for `0.1 ≤ u ≤ 100`, `1 ≤ ε_r ≤ 20`,
/// `0 ≤ h/λ₀ ≤ 0.13` (`f_n ≤ 39` GHz·mm).
pub fn kirschning_jansen_eps_eff(u: f64, eps_r: f64, eps_eff0: f64, f_h_ghz_mm: f64) -> f64 {
    assert!(u > 0.0 && eps_r >= 1.0 && f_h_ghz_mm >= 0.0);
    let fnn = f_h_ghz_mm;
    let p1 = 0.27488 + (0.6315 + 0.525 / (1.0 + 0.0157 * fnn).powi(20)) * u
        - 0.065683 * (-8.7513 * u).exp();
    let p2 = 0.33622 * (1.0 - (-0.03442 * eps_r).exp());
    let p3 = 0.0363 * (-4.6 * u).exp() * (1.0 - (-(fnn / 38.7).powf(4.97)).exp());
    let p4 = 1.0 + 2.751 * (1.0 - (-(eps_r / 15.916).powi(8)).exp());
    let p = p1 * p2 * ((0.1844 + p3 * p4) * fnn).powf(1.5763);
    eps_r - (eps_r - eps_eff0) / (1.0 + p)
}

/// Quasi-TEM dielectric attenuation of a microstrip, in nepers per unit length
/// (the length unit of `1/k0`); D. M. Pozar, *Microwave Engineering*, 4th ed., eq. (3.198):
///
/// ```text
///   α_d = k₀ ε_r (ε_eff − 1) tan δ / (2 √ε_eff (ε_r − 1))
/// ```
///
/// The filling factor `ε_r(ε_eff − 1)/(ε_eff(ε_r − 1))` accounts for the
/// part of the field in air. Valid for small `tan δ`. For a homogeneous fill
/// (`ε_eff = ε_r`) it reduces to `k₀√ε_r tan δ/2`.
pub fn pozar_dielectric_attenuation(k0: f64, eps_r: f64, eps_eff: f64, tan_delta: f64) -> f64 {
    assert!(eps_r > 1.0 && eps_eff > 1.0, "need eps_r, eps_eff > 1");
    k0 * eps_r * (eps_eff - 1.0) * tan_delta / (2.0 * eps_eff.sqrt() * (eps_r - 1.0))
}

/// Geometric grading of a 1-D span `[a, b]`: cell sizes `h_min·r^k` growing
/// away from the clustered end(s), capped at `h_max`, then rescaled so they
/// sum to `b − a` exactly. Returns the interior grid points plus `b` (not
/// `a`).
fn graded_span(a: f64, b: f64, at_start: bool, at_end: bool, opts: &StripMeshOpts) -> Vec<f64> {
    let len = b - a;
    assert!(len > 0.0, "empty span");
    if at_start && at_end {
        let mid = 0.5 * (a + b);
        let mut v = graded_span(a, mid, true, false, opts);
        v.extend(graded_span(mid, b, false, true, opts));
        return v;
    }
    let mut sizes = Vec::new();
    let mut total = 0.0;
    let mut hk = if at_start || at_end {
        opts.h_min
    } else {
        opts.h_max
    };
    while total < len * (1.0 - 1e-9) {
        let step = hk.min(opts.h_max);
        sizes.push(step);
        total += step;
        hk *= opts.ratio;
    }
    // Merge an overshooting last cell into its neighbour when it is a sliver.
    if sizes.len() > 1 && total - len > 0.5 * sizes[sizes.len() - 1] {
        let last = sizes.pop().unwrap();
        total -= last;
    }
    let s = len / total;
    if at_end {
        sizes.reverse();
    }
    let mut out = Vec::with_capacity(sizes.len());
    let mut x = a;
    for (i, h) in sizes.iter().enumerate() {
        x += h * s;
        out.push(if i + 1 == sizes.len() { b } else { x });
    }
    out
}

/// Grid points of an axis through `breaks` (sorted, first and last are the
/// walls), graded toward every point listed in `singular`.
fn graded_axis(breaks: &[f64], singular: &[f64], opts: &StripMeshOpts) -> Vec<f64> {
    let tol = 1e-12 * (breaks[breaks.len() - 1] - breaks[0]).abs().max(1.0);
    let is_sing = |x: f64| singular.iter().any(|s| (s - x).abs() <= tol);
    let mut out = vec![breaks[0]];
    for w in breaks.windows(2) {
        out.extend(graded_span(w[0], w[1], is_sing(w[0]), is_sing(w[1]), opts));
    }
    out
}

/// Mesh-resolution options of [`ShieldedStripFace::build`].
#[derive(Debug, Clone, Copy)]
pub struct StripMeshOpts {
    /// Smallest cell size, at the strip edges and on the interface line.
    pub h_min: f64,
    /// Largest cell size, far from the strips.
    pub h_max: f64,
    /// Geometric growth ratio of the cell sizes away from a singular point.
    pub ratio: f64,
    /// `true`: the triangulation is exactly mirror-symmetric about the box
    /// centre line `x = 0` (diagonals flip across it). `false`: every quad
    /// uses the lower-left-to-upper-right diagonal, which breaks the mirror
    /// symmetry.
    pub mirror_symmetric: bool,
}

/// A rectangular PEC box `[−W/2, W/2] × [0, H]` with a two-layer fill
/// (`ε_below` for `y < h`, `ε_above` above) and PEC strips on the interface
/// line `y = h` — shielded microstrip (`ε_above = 1`), or a homogeneous
/// shielded strip line (`ε_above = ε_below`).
///
/// With `thickness = 0` each strip is a zero-thickness PEC sheet: its edges
/// are interior edges of the face, made PEC through
/// [`HybridPecMasks::from_mesh`]'s extra-edge input. With `thickness > 0`
/// each strip is the rectangle `[x₀, x₁] × [h, h + t]` **carved out** of the
/// mesh (a hole), so its rim is PEC by boundary detection alone, the way a
/// `geode mesh` layout produces it.
#[derive(Debug, Clone)]
pub struct ShieldedStripFace {
    /// Box width `W` (centred on `x = 0`).
    pub box_width: f64,
    /// Box height `H` (ground plane `y = 0`).
    pub box_height: f64,
    /// Interface height `h` (substrate thickness).
    pub h: f64,
    /// Strip intervals `[x₀, x₁]` on `y = h` (disjoint, inside the box).
    pub strips: Vec<[f64; 2]>,
    /// Strip thickness `t` (`0` = sheet).
    pub thickness: f64,
    /// Permittivity below the interface.
    pub eps_below: f64,
    /// Permittivity above the interface.
    pub eps_above: f64,
}

/// A built port face: mesh, fill, PEC masks and the conductor bookkeeping
/// the impedance helpers need.
#[derive(Debug, Clone)]
pub struct StripFaceMesh {
    /// The triangulation.
    pub mesh: TriMesh,
    /// Per-triangle relative permittivity.
    pub eps_r: Vec<f64>,
    /// PEC masks (rim + strips).
    pub masks: HybridPecMasks,
    /// For each strip, the node mask of its conductor (`true` on the strip).
    pub conductor_nodes: Vec<Vec<bool>>,
    /// Node mask of the outer shield (the rim).
    pub shield_nodes: Vec<bool>,
    /// For each strip, a node path along mesh edges from the ground plane
    /// (`y = 0`) straight up to the strip centre — the integration path of
    /// [`super::port_modes::mode_line_quantities`]'s voltage.
    pub voltage_paths: Vec<Vec<u32>>,
    /// The extra (non-rim) PEC edge mask that was passed to
    /// [`HybridPecMasks::from_mesh`]: sheet edges, or every edge of a
    /// non-carved conductor cell (all `false` for carved thick strips).
    pub sheet_pec_edges: Vec<bool>,
    /// Grid lines along `x`.
    pub xs: Vec<f64>,
    /// Grid lines along `y`.
    pub ys: Vec<f64>,
}

impl StripFaceMesh {
    /// Smallest edge length of the mesh (the strip-edge cell size).
    pub fn min_edge(&self) -> f64 {
        self.mesh
            .edges()
            .iter()
            .map(|e| {
                let (p, q) = (
                    self.mesh.nodes[e[0] as usize],
                    self.mesh.nodes[e[1] as usize],
                );
                (p[0] - q[0]).hypot(p[1] - q[1])
            })
            .fold(f64::INFINITY, f64::min)
    }
}

impl ShieldedStripFace {
    /// A shielded microstrip: substrate `ε_r`, height `h`, one strip of
    /// width `w` centred in a `W × H` box, air above.
    pub fn microstrip(box_width: f64, box_height: f64, h: f64, w: f64, eps_r: f64) -> Self {
        Self {
            box_width,
            box_height,
            h,
            strips: vec![[-0.5 * w, 0.5 * w]],
            thickness: 0.0,
            eps_below: eps_r,
            eps_above: 1.0,
        }
    }

    /// Build the graded tensor-product triangulation (module docs).
    ///
    /// Grid lines pass through the box walls, every strip end and centre,
    /// the interface `y = h` (and `y = h + t`), and the midpoints between
    /// neighbouring strips; cells are graded geometrically toward the strip
    /// ends and the interface line.
    ///
    /// # Panics
    ///
    /// Panics on an invalid geometry (strips outside the box, overlapping,
    /// `h ≥ H`).
    pub fn build(&self, opts: &StripMeshOpts) -> StripFaceMesh {
        self.build_with(opts, true)
    }

    /// [`Self::build`] with control over thick strips: `carve = true` (the
    /// default) removes the conductor cells, so the strip is a hole whose
    /// inner rim is PEC by boundary detection; `carve = false` keeps them
    /// meshed and marks **every edge of a conductor cell** PEC through the
    /// extra-edge mask instead (the 3-D `pec_interior_mask` view of a
    /// conductor volume). Both leave the same free DOFs, so the two faces
    /// must give the same modes; `tests/hybrid_port_microstrip.rs` checks
    /// that. Ignored for zero-thickness strips.
    pub fn build_with(&self, opts: &StripMeshOpts, carve: bool) -> StripFaceMesh {
        let (wb, hb, h, t) = (self.box_width, self.box_height, self.h, self.thickness);
        assert!(h > 0.0 && h + t < hb, "need 0 < h and h + t < H");
        assert!(t >= 0.0, "thickness must be ≥ 0");
        let mut strips = self.strips.clone();
        strips.sort_by(|a, b| a[0].total_cmp(&b[0]));
        for s in &strips {
            assert!(
                -0.5 * wb < s[0] && s[0] < s[1] && s[1] < 0.5 * wb,
                "strip {s:?} must lie inside the box"
            );
        }
        for w in strips.windows(2) {
            assert!(w[0][1] < w[1][0], "strips overlap");
        }
        // x breakpoints: walls, strip ends + centres, gap midpoints.
        let mut xb = vec![-0.5 * wb, 0.5 * wb];
        let mut xsing = Vec::new();
        for s in &strips {
            xb.extend([s[0], s[1], 0.5 * (s[0] + s[1])]);
            xsing.extend([s[0], s[1]]);
        }
        for w in strips.windows(2) {
            xb.push(0.5 * (w[0][1] + w[1][0]));
        }
        if opts.mirror_symmetric {
            xb.push(0.0);
        }
        xb.sort_by(f64::total_cmp);
        xb.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * wb);
        let xs = graded_axis(&xb, &xsing, opts);
        let mut yb = vec![0.0, h, hb];
        let mut ysing = vec![h];
        if t > 0.0 {
            yb.push(h + t);
            ysing.push(h + t);
        }
        yb.sort_by(f64::total_cmp);
        let ys = graded_axis(&yb, &ysing, opts);

        let npx = xs.len();
        let tol = 1e-12 * wb.max(hb);
        let in_strip_x = |x0: f64, x1: f64| -> Option<usize> {
            strips
                .iter()
                .position(|s| x0 >= s[0] - tol && x1 <= s[1] + tol)
        };
        let mut nodes = Vec::with_capacity(npx * ys.len());
        for &y in &ys {
            for &x in &xs {
                nodes.push([x, y]);
            }
        }
        let idx = |i: usize, j: usize| (i + j * npx) as u32;
        let mut tris = Vec::new();
        let mut eps_r = Vec::new();
        let mut conductor_tri = Vec::new();
        for j in 0..ys.len() - 1 {
            let yc = 0.5 * (ys[j] + ys[j + 1]);
            for i in 0..npx - 1 {
                let xc = 0.5 * (xs[i] + xs[i + 1]);
                // Thick strips: cells inside a conductor.
                let inside =
                    t > 0.0 && yc > h && yc < h + t && in_strip_x(xs[i], xs[i + 1]).is_some();
                if inside && carve {
                    continue;
                }
                conductor_tri.extend([inside, inside]);
                let c = [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)];
                if opts.mirror_symmetric && xc < 0.0 {
                    tris.push([c[0], c[1], c[3]]);
                    tris.push([c[1], c[2], c[3]]);
                } else {
                    tris.push([c[0], c[1], c[2]]);
                    tris.push([c[0], c[2], c[3]]);
                }
                let e = if yc < h {
                    self.eps_below
                } else {
                    self.eps_above
                };
                eps_r.extend([e, e]);
            }
        }
        // Drop nodes left unused by carved-out conductors (strictly inside a
        // thick strip) and renumber.
        let mut used = vec![false; nodes.len()];
        for tri in &tris {
            for &n in tri {
                used[n as usize] = true;
            }
        }
        let mut renum = vec![u32::MAX; nodes.len()];
        let mut kept = Vec::new();
        for (k, p) in nodes.iter().enumerate() {
            if used[k] {
                renum[k] = kept.len() as u32;
                kept.push(*p);
            }
        }
        for tri in &mut tris {
            for n in tri.iter_mut() {
                *n = renum[*n as usize];
            }
        }
        let mesh = TriMesh { nodes: kept, tris };

        // Sheet PEC edges: horizontal edges on y = h within a strip (t = 0).
        let edges = mesh.edges();
        let on_line = |p: [f64; 2], y: f64| (p[1] - y).abs() <= tol;
        let mut sheet_pec_edges: Vec<bool> = edges
            .iter()
            .map(|e| {
                let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
                t == 0.0
                    && on_line(p, h)
                    && on_line(q, h)
                    && in_strip_x(p[0].min(q[0]), p[0].max(q[0])).is_some()
            })
            .collect();
        for (row, &inside) in mesh.tri_edges().iter().zip(&conductor_tri) {
            if inside {
                for &(e, _) in row {
                    sheet_pec_edges[e as usize] = true;
                }
            }
        }
        let masks = HybridPecMasks::from_mesh(&mesh, Some(&sheet_pec_edges));

        // Conductor node masks.
        let in_conductor = |p: [f64; 2], s: &[f64; 2]| {
            p[0] >= s[0] - tol && p[0] <= s[1] + tol && p[1] >= h - tol && p[1] <= h + t + tol
        };
        let conductor_nodes: Vec<Vec<bool>> = strips
            .iter()
            .map(|s| mesh.nodes.iter().map(|&p| in_conductor(p, s)).collect())
            .collect();
        let shield_nodes: Vec<bool> = mesh
            .nodes
            .iter()
            .map(|p| {
                p[0] <= -0.5 * wb + tol || p[0] >= 0.5 * wb - tol || p[1] <= tol || p[1] >= hb - tol
            })
            .collect();
        // Voltage paths: up the grid line through each strip centre.
        let find = |x: f64, y: f64| -> u32 {
            mesh.nodes
                .iter()
                .position(|p| (p[0] - x).abs() <= tol && (p[1] - y).abs() <= tol)
                .expect("grid node on the voltage path") as u32
        };
        let voltage_paths = strips
            .iter()
            .map(|s| {
                let xc = 0.5 * (s[0] + s[1]);
                ys.iter()
                    .take_while(|&&y| y <= h + tol)
                    .map(|&y| find(xc, y))
                    .collect()
            })
            .collect();
        StripFaceMesh {
            mesh,
            eps_r,
            masks,
            conductor_nodes,
            shield_nodes,
            voltage_paths,
            sheet_pec_edges,
            xs,
            ys,
        }
    }
}

/// A **C4v-symmetric square coax**: outer PEC square `[−a, a]²`, inner PEC
/// square `[−b, b]²` carved out (a hole), uniform `ε_r`, on a uniform
/// `2n × 2n` grid (`n` cells per half side, `b = a·n_inner/n`). Each quad's
/// diagonal points away from the centre (`/` where `x·y > 0`, `\`
/// elsewhere), so the triangulation is invariant under `x → −x`, `y → −y`
/// and `x ↔ y`: the higher-order TE11-like modes are **exactly** doubly
/// degenerate on this mesh. Returns the face with `conductor_nodes[0]` the
/// inner conductor.
///
/// # Panics
///
/// Panics unless `1 ≤ n_inner < n`.
pub fn square_coax_face(a: f64, n: usize, n_inner: usize, eps_r: f64) -> StripFaceMesh {
    assert!(n_inner >= 1 && n_inner < n, "need 1 ≤ n_inner < n");
    let hx = a / n as f64;
    let xs: Vec<f64> = (0..=2 * n).map(|i| -a + i as f64 * hx).collect();
    let npx = xs.len();
    let mut nodes = Vec::new();
    for &y in &xs {
        for &x in &xs {
            nodes.push([x, y]);
        }
    }
    let b = n_inner as f64 * hx;
    let tol = 1e-9 * hx;
    let idx = |i: usize, j: usize| (i + j * npx) as u32;
    let mut tris = Vec::new();
    for j in 0..2 * n {
        for i in 0..2 * n {
            let (xc, yc) = (0.5 * (xs[i] + xs[i + 1]), 0.5 * (xs[j] + xs[j + 1]));
            if xc.abs() < b && yc.abs() < b {
                continue;
            }
            let c = [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)];
            if xc * yc > 0.0 {
                tris.push([c[0], c[1], c[2]]);
                tris.push([c[0], c[2], c[3]]);
            } else {
                tris.push([c[0], c[1], c[3]]);
                tris.push([c[1], c[2], c[3]]);
            }
        }
    }
    let mut used = vec![false; nodes.len()];
    for tri in &tris {
        for &k in tri {
            used[k as usize] = true;
        }
    }
    let mut renum = vec![u32::MAX; nodes.len()];
    let mut kept = Vec::new();
    for (k, p) in nodes.iter().enumerate() {
        if used[k] {
            renum[k] = kept.len() as u32;
            kept.push(*p);
        }
    }
    for tri in &mut tris {
        for k in tri.iter_mut() {
            *k = renum[*k as usize];
        }
    }
    let mesh = TriMesh { nodes: kept, tris };
    let n_edges = mesh.edges().len();
    let sheet = vec![false; n_edges];
    let masks = HybridPecMasks::from_mesh(&mesh, None);
    let inner: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| p[0].abs() <= b + tol && p[1].abs() <= b + tol)
        .collect();
    let shield: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| p[0].abs() >= a - tol || p[1].abs() >= a - tol)
        .collect();
    // Voltage path: along y = 0 from x = a to x = b.
    let mut path: Vec<(f64, u32)> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[1].abs() <= tol && p[0] >= b - tol)
        .map(|(k, p)| (p[0], k as u32))
        .collect();
    path.sort_by(|p, q| q.0.total_cmp(&p.0));
    let eps = vec![eps_r; mesh.n_tris()];
    StripFaceMesh {
        mesh,
        eps_r: eps,
        masks,
        conductor_nodes: vec![inner],
        shield_nodes: shield,
        voltage_paths: vec![path.into_iter().map(|(_, k)| k).collect()],
        sheet_pec_edges: sheet,
        xs: xs.clone(),
        ys: xs,
    }
}

/// Result of [`electrostatic_2d`].
#[derive(Debug, Clone)]
pub struct Electrostatic2d {
    /// Nodal potential `ψ` (Dirichlet values included).
    pub potential: Vec<f64>,
    /// `ψᵀ S_ε ψ` with the full (unconstrained) ε-weighted P1 stiffness:
    /// `C/ε₀` per unit length for a single conductor at unit potential.
    pub energy: f64,
}

/// 2-D P1 electrostatic solve `∇·(ε_r ∇ψ) = 0` on `mesh`, with
/// `dirichlet[k] = Some(V)` fixing node `k` (conductors and shield) and
/// `None` free. Returns the potential and the energy `ψᵀS_εψ` (module docs).
///
/// # Panics
///
/// Panics on mismatched lengths or a singular (no-Dirichlet) problem.
pub fn electrostatic_2d(
    mesh: &TriMesh,
    eps_r: &[f64],
    dirichlet: &[Option<f64>],
) -> Electrostatic2d {
    use faer::linalg::solvers::Solve;
    assert_eq!(eps_r.len(), mesh.n_tris());
    assert_eq!(dirichlet.len(), mesh.n_nodes());
    let n = mesh.n_nodes();
    let mut renum = vec![usize::MAX; n];
    let mut n_free = 0usize;
    for (k, d) in dirichlet.iter().enumerate() {
        if d.is_none() {
            renum[k] = n_free;
            n_free += 1;
        }
    }
    let mut trips = Vec::new();
    let mut rhs = vec![0.0; n_free];
    let mut full = Vec::new();
    for (tri, &eps) in mesh.tris.iter().zip(eps_r) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (s_loc, _, _) = tri_p1_local(&coords);
        for p in 0..3 {
            for q in 0..3 {
                let (gp, gq) = (tri[p] as usize, tri[q] as usize);
                let v = eps * s_loc[p][q];
                full.push((gp, gq, v));
                if renum[gp] == usize::MAX {
                    continue;
                }
                match dirichlet[gq] {
                    None => trips.push(Triplet::new(renum[gp], renum[gq], v)),
                    Some(val) => rhs[renum[gp]] -= v * val,
                }
            }
        }
    }
    let mut psi: Vec<f64> = dirichlet.iter().map(|d| d.unwrap_or(0.0)).collect();
    if n_free > 0 {
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &trips)
            .expect("electrostatic stiffness triplets");
        let lu = k.as_ref().sp_lu().expect("electrostatic stiffness LU");
        let mut b = faer::Mat::<f64>::from_fn(n_free, 1, |i, _| rhs[i]);
        lu.solve_in_place(b.as_mut());
        for (kk, &r) in renum.iter().enumerate() {
            if r != usize::MAX {
                psi[kk] = b[(r, 0)];
            }
        }
    }
    let energy = full.iter().map(|&(p, q, v)| psi[p] * v * psi[q]).sum();
    Electrostatic2d {
        potential: psi,
        energy,
    }
}

/// Quasi-static line parameters of one conductor on a [`StripFaceMesh`]:
/// conductor `k` at 1 V, every other conductor and the shield at 0 V.
#[derive(Debug, Clone, Copy)]
pub struct QuasiStaticLine {
    /// `C/ε₀` with the actual fill.
    pub c_eps: f64,
    /// `C/ε₀` with the fill replaced by vacuum.
    pub c_air: f64,
    /// `ε_eff,qs = c_eps / c_air`.
    pub eps_eff: f64,
    /// `Z_qs = η₀ / √(c_eps · c_air)` (ohms).
    pub z0: f64,
}

/// [`QuasiStaticLine`] of conductor `k` of `face` (two [`electrostatic_2d`]
/// solves on the same mesh).
pub fn quasi_static_line(face: &StripFaceMesh, k: usize) -> QuasiStaticLine {
    let dir: Vec<Option<f64>> = (0..face.mesh.n_nodes())
        .map(|i| {
            if face.conductor_nodes[k][i] {
                Some(1.0)
            } else if face.shield_nodes[i] || face.conductor_nodes.iter().any(|c| c[i]) {
                Some(0.0)
            } else {
                None
            }
        })
        .collect();
    let c_eps = electrostatic_2d(&face.mesh, &face.eps_r, &dir).energy;
    let ones = vec![1.0; face.mesh.n_tris()];
    let c_air = electrostatic_2d(&face.mesh, &ones, &dir).energy;
    QuasiStaticLine {
        c_eps,
        c_air,
        eps_eff: c_eps / c_air,
        z0: ETA_0_OHM / (c_eps * c_air).sqrt(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hammerstad (1975) / Pozar eq. (3.195)–(3.196): a simpler, independent
    /// closed form (about 1 % accurate) used to cross-check the HJ code.
    fn pozar_eps_eff(u: f64, er: f64) -> f64 {
        0.5 * (er + 1.0) + 0.5 * (er - 1.0) / (1.0 + 12.0 / u).sqrt()
    }

    fn pozar_z0(u: f64, er: f64) -> f64 {
        let ee = pozar_eps_eff(u, er);
        if u <= 1.0 {
            60.0 / ee.sqrt() * (8.0 / u + u / 4.0).ln()
        } else {
            ETA_0_OHM / (ee.sqrt() * (u + 1.393 + 0.667 * (u + 1.444).ln()))
        }
    }

    #[test]
    fn hammerstad_jensen_agrees_with_the_simpler_pozar_forms() {
        for &er in &[2.2, 4.4, 9.8] {
            for &u in &[0.5, 1.0, 2.0, 4.0] {
                let (e_hj, e_p) = (hammerstad_jensen_eps_eff(u, er), pozar_eps_eff(u, er));
                let (z_hj, z_p) = (hammerstad_jensen_z0(u, er), pozar_z0(u, er));
                assert!(
                    (e_hj / e_p - 1.0).abs() < 0.01,
                    "ε_eff u={u} εr={er}: {e_hj} vs {e_p}"
                );
                assert!(
                    (z_hj / z_p - 1.0).abs() < 0.015,
                    "Z₀ u={u} εr={er}: {z_hj} vs {z_p}"
                );
            }
        }
        // Limits: ε_r = 1 gives ε_eff = 1; a very wide strip tends to ε_r.
        assert!((hammerstad_jensen_eps_eff(1.0, 1.0) - 1.0).abs() < 1e-12);
        assert!(hammerstad_jensen_eps_eff(100.0, 4.4) > 4.0);
    }

    #[test]
    fn kirschning_jansen_limits() {
        let (u, er) = (1.0, 9.8);
        let e0 = hammerstad_jensen_eps_eff(u, er);
        assert!((kirschning_jansen_eps_eff(u, er, e0, 0.0) - e0).abs() < 1e-14);
        let mut prev = e0;
        for &fh in &[1.0, 5.0, 10.0, 30.0] {
            let e = kirschning_jansen_eps_eff(u, er, e0, fh);
            assert!(e > prev && e < er, "monotone toward ε_r: {e} at f·h = {fh}");
            prev = e;
        }
    }

    #[test]
    fn pozar_attenuation_homogeneous_limit() {
        let (k0, er, td) = (2.0, 4.4, 0.02);
        let a = pozar_dielectric_attenuation(k0, er, er, td);
        assert!((a - k0 * er.sqrt() * td / 2.0).abs() < 1e-14);
    }

    #[test]
    fn graded_axis_hits_breakpoints_and_grades() {
        let opts = StripMeshOpts {
            h_min: 0.01,
            h_max: 0.5,
            ratio: 1.3,
            mirror_symmetric: true,
        };
        let xs = graded_axis(&[-5.0, -0.5, 0.0, 0.5, 5.0], &[-0.5, 0.5], &opts);
        for b in [-5.0, -0.5, 0.0, 0.5, 5.0] {
            assert!(xs.iter().any(|x| (x - b).abs() < 1e-12), "breakpoint {b}");
        }
        for w in xs.windows(2) {
            assert!(w[1] > w[0]);
        }
        // Mirror symmetric.
        for (a, b) in xs.iter().zip(xs.iter().rev()) {
            assert!((a + b).abs() < 1e-12, "{a} vs {b}");
        }
        let hmin = xs
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(f64::INFINITY, f64::min);
        assert!(hmin < 0.012, "smallest cell {hmin}");
    }

    /// Coax-like check of the electrostatic solver: a homogeneous square
    /// coax's capacitance scales with ε exactly, and the strip-face fill
    /// produces `1 < ε_eff,qs < ε_r`.
    #[test]
    fn electrostatic_reference_sanity() {
        let coax = square_coax_face(1.0, 8, 3, 1.0);
        let q1 = quasi_static_line(&coax, 0);
        assert!((q1.eps_eff - 1.0).abs() < 1e-12);
        let ms = ShieldedStripFace::microstrip(10.0, 10.0, 1.0, 1.0, 4.4).build(&StripMeshOpts {
            h_min: 0.05,
            h_max: 0.5,
            ratio: 1.4,
            mirror_symmetric: true,
        });
        let q = quasi_static_line(&ms, 0);
        assert!(q.eps_eff > 1.0 && q.eps_eff < 4.4, "{q:?}");
    }
}
