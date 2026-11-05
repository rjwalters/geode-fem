//! Sampling of piecewise-constant / P1 tetrahedral fields onto a consumer's
//! own cells and points.
//!
//! [`recover_e_field`](crate::assembly::electrostatic::recover_e_field)
//! returns one constant `E` per tetrahedron. Near a convex conductor corner
//! the P1 field is singular (`|E| ~ r^(-1/3)`), so a point sample or a
//! per-tet maximum depends on the mesh, while a volume-weighted *cell
//! average* converges under refinement. This module provides:
//!
//! - [`sample_cell_average`] — overlap-weighted averages of a per-tet field
//!   over axis-aligned boxes ([`Aabb`]). The weight of a tet in a cell is the
//!   exact volume of `tet ∩ box`, obtained by clipping the tet against the
//!   box's six half-spaces.
//! - [`sample_points`] — point location plus the potential `φ` (barycentric
//!   interpolation) and `E = −∇φ` at arbitrary points.
//!
//! Point location uses a private uniform bin grid over the mesh bounding box;
//! each tet is registered in the bins its bounding box overlaps.
//!
//! # Determinism
//!
//! Both functions are serial. Candidate tets are visited in ascending tet
//! index and accumulated per cell in that fixed order, so the output is
//! bit-identical across runs and independent of any thread-pool size.

use crate::assembly::electrostatic::recover_e_field;
use crate::mesh::TetMesh;

/// Barycentric tolerance used by [`sample_points`]: a point is inside a tet
/// when every barycentric coordinate is `>= -BARY_TOL`. A point on a shared
/// face/edge/vertex therefore lies in several tets and resolves to the one
/// with the lowest index.
pub const BARY_TOL: f64 = 1e-10;

/// Maximum number of bins per axis in the point-location grid.
const MAX_BINS_PER_AXIS: usize = 128;

/// Axis-aligned box `[min, max]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    /// Box volume.
    pub fn volume(&self) -> f64 {
        (0..3).map(|d| self.max[d] - self.min[d]).product()
    }
}

/// Result of averaging a per-tet field over one cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellAverage {
    /// Volume of the cell that lies inside the mesh (`Σ w`). Less than the
    /// box volume when the cell is partly outside the mesh.
    pub volume_covered: f64,
    /// Overlap-weighted mean vector `Σ w·E_t / Σ w` (zero if nothing covered).
    pub mean: [f64; 3],
    /// Overlap-weighted mean of the magnitude, `Σ w·|E_t| / Σ w` — the mean
    /// of the norm, not the norm of the mean (zero if nothing covered).
    pub mean_magnitude: f64,
}

/// Potential and field at a located point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointSample {
    /// Index of the containing tet (lowest index on a shared face).
    pub tet: usize,
    /// Interpolated potential.
    pub phi: f64,
    /// Field `E = −∇φ` of the containing tet.
    pub e: [f64; 3],
}

// ---------------------------------------------------------------- geometry

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Signed volume of the tet `(a, b, c, d)`.
fn signed_volume(a: V3, b: V3, c: V3, d: V3) -> f64 {
    dot(cross(sub(b, a), sub(c, a)), sub(d, a)) / 6.0
}

/// Split the triangular prism `p[0..3]` (bottom) / `p[3..6]` (top, `p[3+i]`
/// above `p[i]`) into three tets with consistent quad-face diagonals.
fn prism_to_tets(p: [V3; 6]) -> [[V3; 4]; 3] {
    [
        [p[0], p[1], p[2], p[5]],
        [p[0], p[1], p[4], p[5]],
        [p[0], p[3], p[4], p[5]],
    ]
}

/// Point on segment `a→b` where the signed distances `da >= 0 > db` cross zero.
fn lerp_zero(a: V3, da: f64, b: V3, db: f64) -> V3 {
    let t = da / (da - db);
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

/// Clip a tet to the half-space `{x : normal_sign·(x[axis] − offset) >= 0}`,
/// pushing the (up to three) tets that tile the kept region onto `out`.
fn clip_tet(tet: [V3; 4], axis: usize, offset: f64, sign: f64, out: &mut Vec<[V3; 4]>) {
    let d: [f64; 4] = std::array::from_fn(|i| sign * (tet[i][axis] - offset));
    let inside: Vec<usize> = (0..4).filter(|&i| d[i] >= 0.0).collect();
    let outside: Vec<usize> = (0..4).filter(|&i| d[i] < 0.0).collect();
    let cut = |i: usize, o: usize| lerp_zero(tet[i], d[i], tet[o], d[o]);
    match inside.len() {
        4 => out.push(tet),
        0 => {}
        1 => {
            let a = inside[0];
            out.push([
                tet[a],
                cut(a, outside[0]),
                cut(a, outside[1]),
                cut(a, outside[2]),
            ]);
        }
        3 => {
            let o = outside[0];
            let (a, b, c) = (inside[0], inside[1], inside[2]);
            out.extend(prism_to_tets([
                tet[a],
                tet[b],
                tet[c],
                cut(a, o),
                cut(b, o),
                cut(c, o),
            ]));
        }
        _ => {
            let (a, b) = (inside[0], inside[1]);
            let (o, p) = (outside[0], outside[1]);
            out.extend(prism_to_tets([
                tet[a],
                cut(a, o),
                cut(a, p),
                tet[b],
                cut(b, o),
                cut(b, p),
            ]));
        }
    }
}

/// Exact volume of `tet ∩ cell`.
fn tet_box_overlap_volume(tet: [V3; 4], cell: &Aabb) -> f64 {
    let mut cur = vec![tet];
    let mut next = Vec::new();
    for axis in 0..3 {
        for (offset, sign) in [(cell.min[axis], 1.0), (cell.max[axis], -1.0)] {
            next.clear();
            for t in &cur {
                clip_tet(*t, axis, offset, sign, &mut next);
            }
            std::mem::swap(&mut cur, &mut next);
            if cur.is_empty() {
                return 0.0;
            }
        }
    }
    cur.iter()
        .map(|t| signed_volume(t[0], t[1], t[2], t[3]).abs())
        .sum()
}

// -------------------------------------------------------------- accelerator

/// Uniform bin grid over the mesh bounding box.
struct BinGrid {
    origin: V3,
    inv_size: V3,
    dims: [usize; 3],
    bins: Vec<Vec<u32>>,
}

impl BinGrid {
    fn new(mesh: &TetMesh) -> Self {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for n in &mesh.nodes {
            for d in 0..3 {
                lo[d] = lo[d].min(n[d]);
                hi[d] = hi[d].max(n[d]);
            }
        }
        if mesh.nodes.is_empty() {
            lo = [0.0; 3];
            hi = [0.0; 3];
        }
        // Mean per-axis tet bbox extent sets the bin size.
        let mut ext = [0.0_f64; 3];
        for t in &mesh.tets {
            let (tl, th) = tet_bbox(mesh, t);
            for d in 0..3 {
                ext[d] += th[d] - tl[d];
            }
        }
        let nt = mesh.tets.len().max(1) as f64;
        let mut dims = [1usize; 3];
        let mut inv_size = [0.0_f64; 3];
        for d in 0..3 {
            let len = hi[d] - lo[d];
            let e = ext[d] / nt;
            if len > 0.0 && e > 0.0 {
                dims[d] = ((len / e).ceil() as usize).clamp(1, MAX_BINS_PER_AXIS);
                inv_size[d] = dims[d] as f64 / len;
            }
        }
        let mut grid = BinGrid {
            origin: lo,
            inv_size,
            dims,
            bins: vec![Vec::new(); dims[0] * dims[1] * dims[2]],
        };
        for (i, t) in mesh.tets.iter().enumerate() {
            let (tl, th) = tet_bbox(mesh, t);
            let (b0, b1) = grid.range(tl, th);
            for k in b0[2]..=b1[2] {
                for j in b0[1]..=b1[1] {
                    for ii in b0[0]..=b1[0] {
                        let idx = grid.flat(ii, j, k);
                        grid.bins[idx].push(i as u32);
                    }
                }
            }
        }
        grid
    }

    fn flat(&self, i: usize, j: usize, k: usize) -> usize {
        i + self.dims[0] * (j + self.dims[1] * k)
    }

    fn axis_bin(&self, d: usize, x: f64) -> usize {
        let f = ((x - self.origin[d]) * self.inv_size[d]).floor();
        // NaN compares false and falls through to 0 via the cast.
        f.clamp(0.0, (self.dims[d] - 1) as f64) as usize
    }

    fn range(&self, lo: V3, hi: V3) -> ([usize; 3], [usize; 3]) {
        let b0 = std::array::from_fn(|d| self.axis_bin(d, lo[d]));
        let b1 = std::array::from_fn(|d| self.axis_bin(d, hi[d]));
        (b0, b1)
    }

    /// Candidate tet indices for a box, ascending and deduplicated.
    fn candidates(&self, lo: V3, hi: V3) -> Vec<u32> {
        let (b0, b1) = self.range(lo, hi);
        let mut out = Vec::new();
        for k in b0[2]..=b1[2] {
            for j in b0[1]..=b1[1] {
                for i in b0[0]..=b1[0] {
                    out.extend_from_slice(&self.bins[self.flat(i, j, k)]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn tet_bbox(mesh: &TetMesh, t: &[u32; 4]) -> (V3, V3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &v in t {
        let p = mesh.nodes[v as usize];
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    (lo, hi)
}

fn tet_coords(mesh: &TetMesh, t: &[u32; 4]) -> [V3; 4] {
    std::array::from_fn(|i| mesh.nodes[t[i] as usize])
}

// ------------------------------------------------------------------ public

/// Volume-weighted average of a per-tet field over axis-aligned cells.
///
/// The weight of tet `t` in a cell is the exact volume of `t ∩ cell`.
/// `volume_covered` reports the part of the cell inside the mesh, so a cell
/// straddling the boundary is averaged over its covered part only.
///
/// Deterministic: serial, candidates visited in ascending tet index (see the
/// module docs).
///
/// # Panics
///
/// Panics if `field_per_tet.len() != mesh.n_tets()` or if any cell has
/// `min > max` on some axis.
pub fn sample_cell_average(
    mesh: &TetMesh,
    field_per_tet: &[[f64; 3]],
    cells: &[Aabb],
) -> Vec<CellAverage> {
    assert_eq!(
        field_per_tet.len(),
        mesh.n_tets(),
        "field length {} != tet count {}",
        field_per_tet.len(),
        mesh.n_tets()
    );
    for (i, c) in cells.iter().enumerate() {
        assert!(
            (0..3).all(|d| c.min[d] <= c.max[d]),
            "cell {i} has min > max: {c:?}"
        );
    }
    if cells.is_empty() {
        return Vec::new();
    }
    let grid = BinGrid::new(mesh);
    cells
        .iter()
        .map(|cell| {
            let mut vol = 0.0_f64;
            let mut sum = [0.0_f64; 3];
            let mut sum_mag = 0.0_f64;
            for ti in grid.candidates(cell.min, cell.max) {
                let t = &mesh.tets[ti as usize];
                let (tl, th) = tet_bbox(mesh, t);
                if (0..3).any(|d| th[d] <= cell.min[d] || tl[d] >= cell.max[d]) {
                    continue;
                }
                let coords = tet_coords(mesh, t);
                let w = if (0..3).all(|d| tl[d] >= cell.min[d] && th[d] <= cell.max[d]) {
                    signed_volume(coords[0], coords[1], coords[2], coords[3]).abs()
                } else {
                    tet_box_overlap_volume(coords, cell)
                };
                if w <= 0.0 {
                    continue;
                }
                let e = field_per_tet[ti as usize];
                vol += w;
                for d in 0..3 {
                    sum[d] += w * e[d];
                }
                sum_mag += w * dot(e, e).sqrt();
            }
            if vol > 0.0 {
                CellAverage {
                    volume_covered: vol,
                    mean: [sum[0] / vol, sum[1] / vol, sum[2] / vol],
                    mean_magnitude: sum_mag / vol,
                }
            } else {
                CellAverage {
                    volume_covered: 0.0,
                    mean: [0.0; 3],
                    mean_magnitude: 0.0,
                }
            }
        })
        .collect()
}

/// Locate points in the mesh and evaluate the potential and `E`.
///
/// `φ` is interpolated barycentrically; `E = −∇φ` is the containing tet's
/// constant field from [`recover_e_field`]. Returns `None` for a point
/// outside the mesh. A point on a shared face/edge/vertex (barycentric
/// tolerance [`BARY_TOL`]) resolves to the lowest-index containing tet.
///
/// # Panics
///
/// Panics if `phi.len() != mesh.n_nodes()`.
pub fn sample_points(mesh: &TetMesh, phi: &[f64], points: &[[f64; 3]]) -> Vec<Option<PointSample>> {
    assert_eq!(
        phi.len(),
        mesh.n_nodes(),
        "phi length {} != node count {}",
        phi.len(),
        mesh.n_nodes()
    );
    if points.is_empty() {
        return Vec::new();
    }
    let e_field = recover_e_field(mesh, phi);
    let grid = BinGrid::new(mesh);
    points
        .iter()
        .map(|&p| {
            for ti in grid.candidates(p, p) {
                let t = &mesh.tets[ti as usize];
                let c = tet_coords(mesh, t);
                let vol = signed_volume(c[0], c[1], c[2], c[3]);
                if vol == 0.0 {
                    continue;
                }
                // λ_i = vol(tet with vertex i replaced by p) / vol.
                let lam = [
                    signed_volume(p, c[1], c[2], c[3]) / vol,
                    signed_volume(c[0], p, c[2], c[3]) / vol,
                    signed_volume(c[0], c[1], p, c[3]) / vol,
                    signed_volume(c[0], c[1], c[2], p) / vol,
                ];
                if lam.iter().all(|&l| l >= -BARY_TOL) {
                    let value = (0..4).map(|i| lam[i] * phi[t[i] as usize]).sum();
                    return Some(PointSample {
                        tet: ti as usize,
                        phi: value,
                        e: e_field[ti as usize],
                    });
                }
            }
            None
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    const A: [f64; 3] = [2.0, -1.0, 0.5];

    fn linear_phi(mesh: &TetMesh) -> Vec<f64> {
        mesh.nodes.iter().map(|p| dot(A, *p)).collect()
    }

    fn tet_vol(mesh: &TetMesh, t: usize) -> f64 {
        let c = tet_coords(mesh, &mesh.tets[t]);
        signed_volume(c[0], c[1], c[2], c[3]).abs()
    }

    /// `m³` boxes tiling `[0, side]³` (deliberately not aligned with the mesh).
    fn tiling(m: usize, side: f64) -> Vec<Aabb> {
        let h = side / m as f64;
        let mut cells = Vec::new();
        for k in 0..m {
            for j in 0..m {
                for i in 0..m {
                    let lo = [i as f64 * h, j as f64 * h, k as f64 * h];
                    cells.push(Aabb {
                        min: lo,
                        max: [lo[0] + h, lo[1] + h, lo[2] + h],
                    });
                }
            }
        }
        cells
    }

    #[test]
    fn plane_clip_conserves_volume() {
        let tet = [
            [0.0, 0.0, 0.0],
            [1.0, 0.1, 0.0],
            [0.2, 1.0, 0.1],
            [0.1, 0.3, 1.0],
        ];
        let full = signed_volume(tet[0], tet[1], tet[2], tet[3]).abs();
        // Every vertex-split case: offsets between sorted x coordinates.
        for off in [-0.5, 0.05, 0.15, 0.5, 1.5] {
            let mut lo = Vec::new();
            let mut hi = Vec::new();
            clip_tet(tet, 0, off, 1.0, &mut lo);
            clip_tet(tet, 0, off, -1.0, &mut hi);
            let v: f64 = lo
                .iter()
                .chain(hi.iter())
                .map(|t| signed_volume(t[0], t[1], t[2], t[3]).abs())
                .sum();
            assert!((v - full).abs() < 1e-14, "off={off}: {v} vs {full}");
        }
    }

    #[test]
    fn tiling_conserves_volume_and_flux() {
        let mesh = cube_tet_mesh(4, 1.0);
        let phi = linear_phi(&mesh);
        let e = recover_e_field(&mesh, &phi);
        let avgs = sample_cell_average(&mesh, &e, &tiling(3, 1.0));
        let total: f64 = (0..mesh.n_tets()).map(|t| tet_vol(&mesh, t)).sum();
        let covered: f64 = avgs.iter().map(|a| a.volume_covered).sum();
        assert!((covered - total).abs() <= 1e-12 * total);
        for d in 0..3 {
            let lhs: f64 = avgs.iter().map(|a| a.volume_covered * a.mean[d]).sum();
            let rhs: f64 = (0..mesh.n_tets())
                .map(|t| tet_vol(&mesh, t) * e[t][d])
                .sum();
            assert!((lhs - rhs).abs() < 1e-12, "axis {d}: {lhs} vs {rhs}");
        }
    }

    #[test]
    fn uniform_field_and_potential_reproduced() {
        let mesh = cube_tet_mesh(4, 1.0);
        let phi = linear_phi(&mesh);
        let e = recover_e_field(&mesh, &phi);
        for a in sample_cell_average(&mesh, &e, &tiling(3, 1.0)) {
            for d in 0..3 {
                assert!((a.mean[d] + A[d]).abs() < 1e-12);
            }
            assert!((a.mean_magnitude - dot(A, A).sqrt()).abs() < 1e-12);
        }
        let pts = [[0.31, 0.42, 0.53], [0.9, 0.1, 0.77], [0.5, 0.5, 0.5]];
        for (p, s) in pts.iter().zip(sample_points(&mesh, &phi, &pts)) {
            let s = s.expect("interior point");
            assert!((s.phi - dot(A, *p)).abs() < 1e-12);
            for d in 0..3 {
                assert!((s.e[d] + A[d]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn boundary_straddle_and_outside_cells() {
        let mesh = cube_tet_mesh(3, 1.0);
        let e = vec![[1.0, 2.0, 2.0]; mesh.n_tets()];
        let cells = [
            Aabb {
                min: [0.5, 0.0, 0.0],
                max: [1.5, 1.0, 1.0],
            },
            Aabb {
                min: [2.0, 2.0, 2.0],
                max: [3.0, 3.0, 3.0],
            },
            // Faces coincide with mesh faces; touching-only neighbours add 0.
            Aabb {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
            },
        ];
        let r = sample_cell_average(&mesh, &e, &cells);
        assert!((r[0].volume_covered - 0.5).abs() < 1e-12);
        assert!(r[0].volume_covered < cells[0].volume());
        assert!((r[0].mean[1] - 2.0).abs() < 1e-12);
        assert!((r[0].mean_magnitude - 3.0).abs() < 1e-12);
        assert_eq!(r[1].volume_covered, 0.0);
        assert_eq!(r[1].mean, [0.0; 3]);
        assert_eq!(r[1].mean_magnitude, 0.0);
        assert!((r[2].volume_covered - 1.0).abs() < 1e-12);
    }

    #[test]
    fn outside_point_none_and_shared_face_lowest_index() {
        let mesh = cube_tet_mesh(2, 1.0);
        let phi = linear_phi(&mesh);
        let out = sample_points(&mesh, &phi, &[[1.5, 0.5, 0.5], [-0.1, 0.0, 0.0]]);
        assert!(out.iter().all(Option::is_none));

        // Every mesh node is shared by several tets.
        for (ni, &p) in mesh.nodes.iter().enumerate() {
            let expected = mesh
                .tets
                .iter()
                .position(|t| t.contains(&(ni as u32)))
                .unwrap();
            let s = sample_points(&mesh, &phi, &[p])[0].unwrap();
            assert_eq!(s.tet, expected, "node {ni}");
            assert!((s.phi - phi[ni]).abs() < 1e-12);
        }
    }

    #[test]
    fn empty_inputs_and_bad_cell() {
        let mesh = cube_tet_mesh(1, 1.0);
        let phi = linear_phi(&mesh);
        let e = recover_e_field(&mesh, &phi);
        assert!(sample_cell_average(&mesh, &e, &[]).is_empty());
        assert!(sample_points(&mesh, &phi, &[]).is_empty());
    }

    #[test]
    #[should_panic(expected = "min > max")]
    fn inverted_cell_panics() {
        let mesh = cube_tet_mesh(1, 1.0);
        let e = vec![[0.0; 3]; mesh.n_tets()];
        let bad = Aabb {
            min: [1.0, 0.0, 0.0],
            max: [0.0, 1.0, 1.0],
        };
        sample_cell_average(&mesh, &e, &[bad]);
    }

    #[test]
    fn repeated_calls_are_bit_identical() {
        let mesh = cube_tet_mesh(5, 1.0);
        let e: Vec<[f64; 3]> = (0..mesh.n_tets())
            .map(|i| [(i as f64).sin(), (i as f64 * 0.7).cos(), 0.1 * i as f64])
            .collect();
        let cells = tiling(4, 1.0);
        let bits = |v: &[CellAverage]| -> Vec<u64> {
            v.iter()
                .flat_map(|a| {
                    [
                        a.volume_covered.to_bits(),
                        a.mean[0].to_bits(),
                        a.mean[1].to_bits(),
                        a.mean[2].to_bits(),
                        a.mean_magnitude.to_bits(),
                    ]
                })
                .collect()
        };
        let first = bits(&sample_cell_average(&mesh, &e, &cells));
        let again = bits(&sample_cell_average(&mesh, &e, &cells));
        assert_eq!(first, again);
        // Result is also independent of the calling thread.
        let (m2, e2, c2) = (mesh.clone(), e.clone(), cells.clone());
        let threaded = std::thread::spawn(move || bits(&sample_cell_average(&m2, &e2, &c2)))
            .join()
            .unwrap();
        assert_eq!(first, threaded);
    }

    /// Synthetic corner-singular field `|E| = r^(-1/3)` (r = distance of the
    /// tet centroid from the corner at the origin) sampled on two refinement
    /// levels: the per-tet maximum grows like `h^(-1/3)` while the 2×2×2
    /// cell averages barely move. This is a synthetic stand-in for a solved
    /// stepped-electrode problem, not a Poisson solve.
    #[test]
    fn cell_average_converges_where_tet_max_does_not() {
        let field = |mesh: &TetMesh| -> Vec<[f64; 3]> {
            mesh.tets
                .iter()
                .map(|t| {
                    let c = tet_coords(mesh, t);
                    let r = (0..3)
                        .map(|d| ((c[0][d] + c[1][d] + c[2][d] + c[3][d]) / 4.0).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    [r.powf(-1.0 / 3.0), 0.0, 0.0]
                })
                .collect()
        };
        let cells = tiling(2, 1.0);
        let run = |n: usize| {
            let mesh = cube_tet_mesh(n, 1.0);
            let e = field(&mesh);
            let tet_max = e.iter().map(|v| v[0]).fold(0.0_f64, f64::max);
            let avgs: Vec<f64> = sample_cell_average(&mesh, &e, &cells)
                .iter()
                .map(|a| a.mean_magnitude)
                .collect();
            (tet_max, avgs)
        };
        let (max_c, avg_c) = run(4);
        let (max_f, avg_f) = run(8);
        let max_change = (max_f - max_c).abs() / max_c;
        let avg_change = avg_c
            .iter()
            .zip(&avg_f)
            .map(|(a, b)| (a - b).abs() / a)
            .fold(0.0_f64, f64::max);
        assert!(
            avg_change < max_change,
            "cell-average change {avg_change} should be below per-tet max change {max_change}"
        );
    }
}
