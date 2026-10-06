//! The **canonical wave-port mode gauge** (issue #888): a sign (and, inside
//! a degenerate cluster, a basis) for every cross-section mode of a
//! geometric wave port, read from the mode's continuous overlap with fixed
//! reference fields in the port's canonical in-plane frame.
//!
//! # Why
//!
//! A cross-section eigenvector is defined up to sign, and S-parameters
//! between two ports carry the product of the two ports' signs. A port's
//! sign therefore has to be a property of the **geometry** — the same for
//! the same cross-section however the face is meshed, listed or wound — or
//! the cross-port entries (`S21`, `S12`) of a mode turn by 180° from one
//! mesh to the next while every magnitude, reciprocity and passivity check
//! still passes.
//!
//! The previous port-face gauge (#300, `gauge_fix_eigenvector`) had two
//! holes, both measured in #888:
//!
//! 1. Its reference fields lived in the face's in-plane frame `(u, v)`,
//!    which [`super::project_port_face`] took from the **first triangle** of
//!    the face list (its first edge and its winding). Two ports of one guide
//!    got rotated or mirrored frames, so a mode's reference — and its sign —
//!    differed between them. [`super::project_port_face`] now builds a
//!    canonical frame from the plane alone.
//! 2. Its floor (`10⁻⁶` of the Cauchy–Schwarz ceiling) let a reference that
//!    is orthogonal to the continuous mode decide the sign through
//!    discretization noise (TE₂₀ and TE₀₁ projected onto `ŷ sin(πx/a)`).
//!
//! # The rule
//!
//! The overlap of a mode `e` with a reference field `F` is the continuous
//! functional `∫ e · F dA`, integrated by a degree-4 rule over the face, in
//! the face's canonical frame and its bounding box `(sx, sy) ∈ [0, 1]²`
//! ([`reference_field`]: the six fields of the #300 list, then the TE_mn
//! transverse shapes up to `m, n ≤` [`REF_MAX_INDEX`]). Its relative size is
//! `|∫ e·F| / (‖e‖·‖F‖)`. For each mode the gauge takes the **first**
//! reference, in list order, whose relative overlap is at least
//! [`GAUGE_FLOOR`] and at least [`GAUGE_LEAD_RATIO`] of the largest relative
//! overlap of any reference with that mode, and makes it positive. A
//! matched reference overlaps at `O(1)`, an orthogonal one at the
//! discretization error, so the selected reference — and the sign — is a
//! property of the continuous mode. A mode that no reference reaches is
//! [`EigenError::UngaugableMode`]: there is no silent fallback.
//!
//! # Degenerate clusters
//!
//! A continuously degenerate pair (TE₁₀ / TE₀₁ of a square guide, TE₂₀ /
//! TE₀₁ of a `2 × 1` guide) has no preferred basis, and the discretization
//! splits it at `O(h²)` into a mesh-dependent one: on a structured square
//! face the discrete modes are the diagonal combinations `TE₁₀ ± TE₀₁`. Two
//! consecutive modes are one cluster when their relative cutoff gap is at
//! round-off ([`DEGENERATE_EXACT_REL_TOL`]), or below
//! [`DEGENERATE_CANDIDATE_REL_TOL`] and shrinking under refinement
//! ([`DEGENERATE_CONVERGENCE_RATIO`]; the projected face is refined once
//! and re-solved).
//! The gauge builds the cluster's basis one direction at a time with the
//! same rule: the first reference that reaches the floor and the lead ratio
//! **within the part of the cluster not yet used** fixes the next direction
//! (the combination of the members that overlaps it most, made positive);
//! the next reference is then read in the orthogonal complement. The result
//! is an orthogonal rotation of an M-orthonormal set, so it stays
//! M-orthonormal. The members share the cluster's mean cutoff, so the
//! port's modal term over the cluster is basis-independent and the choice
//! only fixes the S-parameter basis. A cluster cut by the
//! requested mode count is completed from the further modes of the same
//! solve before its basis is chosen, so the kept modes are the canonical
//! ones (the first canonical directions), not an arbitrary member.

use crate::analytic::waveguide::{TRI_LOCAL_EDGES, TRI_QUAD_DEG4, TriMesh, WaveguideModeProfile};
use crate::eigen::dense::EigenError;

/// Relative overlap floor of the canonical gauge: a reference counts only
/// above 1 % of the Cauchy–Schwarz ceiling `‖e‖·‖F‖`.
pub const GAUGE_FLOOR: f64 = 1e-2;

/// A reference counts only if its relative overlap is at least this
/// fraction of the mode's largest relative overlap with any reference.
///
/// Discretization noise in a continuously orthogonal reference sits far
/// below any matched overlap (`O(h²)` against `O(1)`), so it never reaches
/// half the best one. Among references that do, the list order decides,
/// which keeps the choice stable when two matched references are close.
pub const GAUGE_LEAD_RATIO: f64 = 0.5;

/// Relative cutoff gap ([`relative_gap`]) at or below which two consecutive
/// modes are always one degenerate cluster: an exact (symmetry) degeneracy,
/// at round-off.
pub const DEGENERATE_EXACT_REL_TOL: f64 = 1e-6;

/// Relative cutoff gap ([`relative_gap`]) up to which two consecutive modes
/// are a **candidate** degenerate pair, confirmed by refinement
/// ([`DEGENERATE_CONVERGENCE_RATIO`]).
///
/// A continuously degenerate pair is split by the discretization at
/// `O(h²)`: measured 1.2 % for TE₁₀ / TE₀₁ on a 6 × 6 structured square
/// face (whose diagonals make the discrete modes the diagonal combinations
/// `TE₁₀ ± TE₀₁`), 0.19 % on a Gmsh square face at `lc = b/5`, 0.065 % for
/// TE₂₀ / TE₀₁ on the 8 × 4 face of a `2 × 1` guide.
pub const DEGENERATE_CANDIDATE_REL_TOL: f64 = 5e-2;

/// A candidate pair is degenerate when its relative gap on the uniformly
/// refined face (`h/2`) is at most this fraction of the gap on the face:
/// a discretization split shrinks as `O(h²)` (ratio ≈ 1/4), a physical one
/// stays (ratio ≈ 1).
pub const DEGENERATE_CONVERGENCE_RATIO: f64 = 0.5;

/// Relative gap `|b − a| / max(|a|, |b|)` of two cutoffs `k_c²`.
pub fn relative_gap(a: f64, b: f64) -> f64 {
    let m = a.abs().max(b.abs());
    if m == 0.0 { 0.0 } else { (b - a).abs() / m }
}

/// Clusters `[start, end)` of `n` modes from the links `links[i]` (mode
/// `i` and mode `i + 1` are degenerate; `links.len() = n − 1`).
pub(crate) fn clusters_from_links(n: usize, links: &[bool]) -> Vec<(usize, usize)> {
    let mut clusters = Vec::new();
    let mut start = 0;
    for j in 1..=n {
        if j == n || !links[j - 1] {
            clusters.push((start, j));
            start = j;
        }
    }
    clusters
}

/// Highest TE_mn index of the extended reference list.
pub const REF_MAX_INDEX: usize = 6;

/// The six #300 reference fields plus one x- and one y-directed TE_mn
/// shape for every `0 ≤ m, n ≤ REF_MAX_INDEX`.
pub const N_REFERENCE_FIELDS: usize = 6 + 2 * (REF_MAX_INDEX + 1) * (REF_MAX_INDEX + 1);

/// Reference field `r` at the in-box point `(sx, sy) ∈ [0, 1]²` of the
/// port's canonical frame.
///
/// - `r < 6`: the #300 list, in its order: `ŷ sin(πsx)`, `ŷ sin(2πsx)`,
///   `x̂ sin(πsy)`, `x̂ sin(2πsy)`, `x̂`, `ŷ`.
/// - `r ≥ 6`: for `(m, n)` in row-major order, the TE_mn transverse shapes
///   `x̂ cos(mπsx) sin(nπsy)` then `ŷ sin(mπsx) cos(nπsy)`. A shape that
///   vanishes identically (`n = 0` for x, `m = 0` for y) has zero norm and
///   is skipped.
pub fn reference_field(r: usize, sx: f64, sy: f64) -> [f64; 2] {
    let pi = std::f64::consts::PI;
    match r {
        0 => [0.0, (pi * sx).sin()],
        1 => [0.0, (2.0 * pi * sx).sin()],
        2 => [(pi * sy).sin(), 0.0],
        3 => [(2.0 * pi * sy).sin(), 0.0],
        4 => [1.0, 0.0],
        5 => [0.0, 1.0],
        _ => {
            let k = r - 6;
            let (mn, comp) = (k / 2, k % 2);
            let (m, n) = (
                (mn / (REF_MAX_INDEX + 1)) as f64,
                (mn % (REF_MAX_INDEX + 1)) as f64,
            );
            if comp == 0 {
                [(m * pi * sx).cos() * (n * pi * sy).sin(), 0.0]
            } else {
                [0.0, (m * pi * sx).sin() * (n * pi * sy).cos()]
            }
        }
    }
}

/// Quadrature of a port face for the gauge: weight and **in-box**
/// coordinate `(sx, sy) ∈ [0, 1]²` of every sample.
#[derive(Debug, Clone, Default)]
pub(crate) struct GaugeQuadrature {
    /// Quadrature weights (area units).
    pub(crate) weights: Vec<f64>,
    /// Sample points, normalized to the face's bounding box.
    pub(crate) points: Vec<[f64; 2]>,
}

impl GaugeQuadrature {
    /// The bounding box `(lo, extent)` of `mesh`'s nodes.
    pub(crate) fn bounding_box(mesh: &TriMesh) -> ([f64; 2], [f64; 2]) {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &mesh.nodes {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, [0, 1].map(|k| (hi[k] - lo[k]).max(f64::EPSILON)))
    }
}

/// Coefficients of the canonical modes (issue #888, module docs).
///
/// `fields[j][q]` is mode `j`'s transverse field at sample `q` of
/// `quad`; the modes are M-orthonormal (`∫ e_i·e_j dA = δ_ij`), `k_c`
/// ascending, partitioned into consecutive degenerate `clusters`
/// (`[start, end)`, covering `0..fields.len()`). Returns, for each of the first
/// `n_keep` slots, the coefficient vector `c` (length `fields.len()`) of
/// the canonical mode `Σ_j c_j e_j`. A mode outside every degenerate
/// cluster gets `±1` on itself.
///
/// # Errors
///
/// [`EigenError::UngaugableMode`] if no reference reaches a slot.
///
/// # Panics
///
/// Panics if `n_keep > fields.len()` or the sample counts differ.
pub(crate) fn canonical_coefficients(
    quad: &GaugeQuadrature,
    fields: &[Vec<[f64; 2]>],
    clusters: &[(usize, usize)],
    n_keep: usize,
) -> Result<Vec<Vec<f64>>, EigenError> {
    let n = fields.len();
    assert!(n_keep <= n, "n_keep exceeds the mode count");
    let nq = quad.weights.len();
    assert!(fields.iter().all(|f| f.len() == nq), "field sample count");

    // Overlaps g[r][j] = ∫ e_j·F_r and reference norms ‖F_r‖. Only the
    // modes of clusters that start below n_keep are needed.
    let n_used = clusters
        .iter()
        .filter(|c| c.0 < n_keep)
        .map(|c| c.1)
        .max()
        .unwrap_or(0);
    let mut g = vec![vec![0.0_f64; n_used]; N_REFERENCE_FIELDS];
    let mut f_norm = vec![0.0_f64; N_REFERENCE_FIELDS];
    for (r, (gr, fr)) in g.iter_mut().zip(f_norm.iter_mut()).enumerate() {
        let mut n2 = 0.0_f64;
        for (q, (&w, &[sx, sy])) in quad.weights.iter().zip(&quad.points).enumerate() {
            let f = reference_field(r, sx, sy);
            n2 += w * (f[0] * f[0] + f[1] * f[1]);
            if f == [0.0, 0.0] {
                continue;
            }
            for (j, gj) in gr.iter_mut().enumerate() {
                let e = fields[j][q];
                *gj += w * (e[0] * f[0] + e[1] * f[1]);
            }
        }
        *fr = n2.sqrt();
    }

    let mut out: Vec<Vec<f64>> = Vec::with_capacity(n_keep);
    for &(s, e) in clusters.iter().filter(|c| c.0 < n_keep) {
        let size = e - s;
        // Directions chosen so far (orthonormal, in cluster coordinates).
        let mut dirs: Vec<Vec<f64>> = Vec::with_capacity(size);
        let mut used = [false; N_REFERENCE_FIELDS];
        while dirs.len() < size && s + dirs.len() < n_keep {
            // Each reference's overlap with the unused part of the cluster.
            let residual = |r: usize| -> Vec<f64> {
                let mut h: Vec<f64> = g[r][s..e].to_vec();
                for d in &dirs {
                    let p: f64 = d.iter().zip(&h).map(|(a, b)| a * b).sum();
                    h.iter_mut().zip(d).for_each(|(x, dk)| *x -= p * dk);
                }
                h
            };
            let rel: Vec<f64> = (0..N_REFERENCE_FIELDS)
                .map(|r| {
                    if used[r] || f_norm[r] <= 0.0 {
                        return 0.0;
                    }
                    let h = residual(r);
                    h.iter().map(|x| x * x).sum::<f64>().sqrt() / f_norm[r]
                })
                .collect();
            let best = rel.iter().copied().fold(0.0_f64, f64::max);
            let threshold = GAUGE_FLOOR.max(GAUGE_LEAD_RATIO * best);
            let Some(r) = (0..N_REFERENCE_FIELDS).find(|&r| rel[r] >= threshold && rel[r] > 0.0)
            else {
                return Err(EigenError::UngaugableMode {
                    mode: s + dirs.len(),
                    best_rel_proj: best,
                });
            };
            used[r] = true;
            let h = residual(r);
            let norm = h.iter().map(|x| x * x).sum::<f64>().sqrt();
            let d: Vec<f64> = h.iter().map(|x| x / norm).collect();
            let mut c = vec![0.0_f64; n];
            c[s..e].copy_from_slice(&d);
            out.push(c);
            dirs.push(d);
        }
    }
    Ok(out)
}

/// The first `n_keep` canonical modes ([`canonical_coefficients`]) of the
/// M-orthonormal mode vectors `vectors` (any DOF layout; their fields at
/// the samples of `quad` are `fields`), with cutoffs `lambda` (`λ`
/// ascending, partitioned into degenerate `clusters`): per slot, the
/// slot's cutoff (the cluster's mean inside a degenerate cluster) and the
/// combined vector. Shared by the p=1 ([`gauge_whitney_modes`]) and p=2
/// (issue #894) port faces.
///
/// # Errors
///
/// As [`canonical_coefficients`].
pub(crate) fn canonical_modes(
    quad: &GaugeQuadrature,
    fields: &[Vec<[f64; 2]>],
    vectors: &[&[f64]],
    lambda: &[f64],
    clusters: &[(usize, usize)],
    n_keep: usize,
) -> Result<Vec<(f64, Vec<f64>)>, EigenError> {
    let coeffs = canonical_coefficients(quad, fields, clusters, n_keep)?;
    // A degenerate cluster's members share its mean cutoff, so the port's
    // modal term over the cluster (`Σ jβ f fᵀ`) does not depend on the
    // basis chosen inside it.
    let mut lambda = lambda.to_vec();
    for &(s, e) in clusters.iter().filter(|c| c.1 - c.0 > 1) {
        let mean = lambda[s..e].iter().sum::<f64>() / (e - s) as f64;
        lambda[s..e].iter_mut().for_each(|l| *l = mean);
    }
    let len = vectors.first().map_or(0, |v| v.len());
    Ok(coeffs
        .iter()
        .enumerate()
        .map(|(slot, c)| {
            let mut out = vec![0.0_f64; len];
            for (cj, v) in c.iter().zip(vectors) {
                if *cj != 0.0 {
                    out.iter_mut().zip(*v).for_each(|(x, y)| *x += cj * y);
                }
            }
            (lambda[slot], out)
        })
        .collect())
}

/// Gauge the lowest-order (Whitney) modes of a projected port face (issue
/// #888): `modes` are the M-orthonormal profiles of
/// `solve_waveguide_modes_ungauged`, `λ` ascending (the requested ones,
/// then any further ones of the same solve), partitioned into degenerate
/// `clusters`; returns the first `n_keep` canonical modes
/// ([`canonical_modes`]), each with the cutoff of its slot (the
/// cluster's mean inside a degenerate cluster).
///
/// # Errors
///
/// As [`canonical_coefficients`].
pub(crate) fn gauge_whitney_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    modes: &[WaveguideModeProfile],
    clusters: &[(usize, usize)],
    n_keep: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let vectors: Vec<&[f64]> = modes.iter().map(|m| m.e_edges.as_slice()).collect();
    let (quad, fields) = whitney_samples(mesh, edges, &vectors);
    let lambda: Vec<f64> = modes.iter().map(|m| m.lambda).collect();
    Ok(
        canonical_modes(&quad, &fields, &vectors, &lambda, clusters, n_keep)?
            .into_iter()
            .enumerate()
            .map(|(slot, (lambda, e_edges))| WaveguideModeProfile {
                k_c: if lambda == modes[slot].lambda {
                    modes[slot].k_c
                } else {
                    lambda.max(0.0).sqrt()
                },
                lambda,
                e_edges,
            })
            .collect(),
    )
}

/// Degree-4 quadrature of the face `mesh` and the Whitney fields of the
/// edge vectors `vectors` (each indexed by `edges`, the mesh's lower-first
/// edge table) at its samples.
pub(crate) fn whitney_samples(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    vectors: &[&[f64]],
) -> (GaugeQuadrature, Vec<Vec<[f64; 2]>>) {
    let index: std::collections::HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    let (lo, ext) = GaugeQuadrature::bounding_box(mesh);
    let mut quad = GaugeQuadrature::default();
    let mut fields: Vec<Vec<[f64; 2]>> = vec![Vec::new(); vectors.len()];
    for tri in &mesh.tris {
        let c = tri.map(|n| mesh.nodes[n as usize]);
        let (grad, _, _, area) = crate::analytic::waveguide::tri_bary_grads(&c);
        // (global edge, sign) of the three local edges.
        let local = TRI_LOCAL_EDGES.map(|(a, b)| {
            let (na, nb) = (tri[a], tri[b]);
            let (key, sign) = if na < nb {
                ((na, nb), 1.0)
            } else {
                ((nb, na), -1.0)
            };
            (index[&key], sign, a, b)
        });
        for row in TRI_QUAD_DEG4.iter() {
            let lam = [row[0], row[1], row[2]];
            quad.weights.push(row[3] * area);
            let x = [0, 1].map(|k| {
                let xk = lam[0] * c[0][k] + lam[1] * c[1][k] + lam[2] * c[2][k];
                (xk - lo[k]) / ext[k]
            });
            quad.points.push(x);
            // Whitney N = λ_a∇λ_b − λ_b∇λ_a for the local edge (a, b).
            let shapes = local.map(|(_, sign, a, b)| {
                [0, 1].map(|k| sign * (lam[a] * grad[b][k] - lam[b] * grad[a][k]))
            });
            for (vec, f) in vectors.iter().zip(fields.iter_mut()) {
                let mut e = [0.0_f64; 2];
                for (l, sh) in local.iter().zip(&shapes) {
                    let v = vec[l.0];
                    e[0] += v * sh[0];
                    e[1] += v * sh[1];
                }
                f.push(e);
            }
        }
    }
    (quad, fields)
}

/// The canonical sign (`±1`) of one mode (issue #888, the rule of the
/// module docs for a mode outside any cluster), from its field samples
/// `re` and, for a complex mode, `im`. The overlap with a reference is then
/// complex; its larger part (real or imaginary) is made positive. Used by
/// the hybrid ports at their first frequency, whose modes are otherwise
/// signed by their largest edge DOF (a mesh-dependent choice).
///
/// # Errors
///
/// [`EigenError::UngaugableMode`] (with index `mode`) if no reference
/// reaches the mode.
pub(crate) fn reference_sign(
    quad: &GaugeQuadrature,
    re: &[[f64; 2]],
    im: Option<&[[f64; 2]]>,
    mode: usize,
) -> Result<f64, EigenError> {
    let nq = quad.weights.len();
    let field = |q: usize| (re[q], im.map_or([0.0, 0.0], |f| f[q]));
    let e_norm = (0..nq)
        .map(|q| {
            let (a, b) = field(q);
            quad.weights[q] * (a[0] * a[0] + a[1] * a[1] + b[0] * b[0] + b[1] * b[1])
        })
        .sum::<f64>()
        .sqrt();
    if e_norm <= f64::MIN_POSITIVE {
        return Ok(1.0);
    }
    let overlaps: Vec<([f64; 2], f64)> = (0..N_REFERENCE_FIELDS)
        .map(|r| {
            let (mut p, mut f2) = ([0.0_f64; 2], 0.0_f64);
            for q in 0..nq {
                let [sx, sy] = quad.points[q];
                let f = reference_field(r, sx, sy);
                let w = quad.weights[q];
                let (a, b) = field(q);
                p[0] += w * (a[0] * f[0] + a[1] * f[1]);
                p[1] += w * (b[0] * f[0] + b[1] * f[1]);
                f2 += w * (f[0] * f[0] + f[1] * f[1]);
            }
            let rel = if f2 > 0.0 {
                p[0].hypot(p[1]) / (e_norm * f2.sqrt())
            } else {
                0.0
            };
            (p, rel)
        })
        .collect();
    let best = overlaps.iter().fold(0.0_f64, |a, o| a.max(o.1));
    let threshold = GAUGE_FLOOR.max(GAUGE_LEAD_RATIO * best);
    let Some((p, _)) = overlaps.iter().find(|o| o.1 >= threshold && o.1 > 0.0) else {
        return Err(EigenError::UngaugableMode {
            mode,
            best_rel_proj: best,
        });
    };
    let lead = if p[0].abs() >= p[1].abs() { p[0] } else { p[1] };
    Ok(if lead < 0.0 { -1.0 } else { 1.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::{
        rect_pec_interior_edges, rect_tri_mesh, solve_waveguide_modes_ungauged,
    };

    /// The lowest `n` raw modes (and the further ones of the pass) of a
    /// `w × h` rectangle, `nx × ny`.
    fn rect_modes(
        nx: usize,
        ny: usize,
        w: f64,
        h: f64,
        n: usize,
    ) -> (TriMesh, Vec<[u32; 2]>, Vec<WaveguideModeProfile>) {
        let mesh = rect_tri_mesh(nx, ny, w, h);
        let (edges, mask) = rect_pec_interior_edges(&mesh, w, h);
        let (mut m, more) = solve_waveguide_modes_ungauged(&mesh, &edges, &mask, n, None).unwrap();
        m.extend(more);
        (mesh, edges, m)
    }

    fn max_diff(a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b)
            .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()))
    }

    /// Negating any input mode, or rotating a degenerate cluster by any
    /// angle, leaves the canonical modes unchanged.
    #[test]
    fn canonical_modes_ignore_input_signs_and_cluster_rotations() {
        // 2 × 1: TE10, then the TE20 / TE01 pair (discretization-split).
        let (mesh, edges, modes) = rect_modes(8, 4, 2.0, 1.0, 3);
        let clusters = [(0, 1), (1, 3)];
        let base = gauge_whitney_modes(&mesh, &edges, &modes[..3], &clusters, 3).unwrap();
        // The cluster members share the mean cutoff.
        let mean = 0.5 * (modes[1].lambda + modes[2].lambda);
        assert_eq!((base[1].lambda, base[2].lambda), (mean, mean));
        assert_eq!(base[0].lambda, modes[0].lambda);
        for (flip, angle) in [
            ([-1.0, 1.0, -1.0], 0.0),
            ([1.0, -1.0, 1.0], 0.7),
            ([-1.0; 3], 2.4),
        ] {
            let mut m: Vec<WaveguideModeProfile> = modes[..3].to_vec();
            for (mi, f) in m.iter_mut().zip(flip) {
                mi.e_edges.iter_mut().for_each(|x| *x *= f);
            }
            let (c, s) = (f64::cos(angle), f64::sin(angle));
            let (e1, e2) = (m[1].e_edges.clone(), m[2].e_edges.clone());
            for k in 0..e1.len() {
                m[1].e_edges[k] = c * e1[k] - s * e2[k];
                m[2].e_edges[k] = s * e1[k] + c * e2[k];
            }
            let got = gauge_whitney_modes(&mesh, &edges, &m, &clusters, 3).unwrap();
            for (g, b) in got.iter().zip(&base) {
                let d = max_diff(&g.e_edges, &b.e_edges);
                assert!(d < 1e-12, "flip {flip:?} angle {angle}: moved by {d:e}");
            }
        }
        // The canonical cluster basis: TE20 (ŷ sin 2πsx) first, then TE01.
        let quad_fields = |e: &[f64]| whitney_samples(&mesh, &edges, &[e]);
        let rel = |e: &[f64], r: usize| {
            let (q, f) = quad_fields(e);
            let (mut p, mut n2) = (0.0, 0.0);
            for (k, (w, x)) in q.weights.iter().zip(&q.points).enumerate() {
                let fr = reference_field(r, x[0], x[1]);
                p += w * (f[0][k][0] * fr[0] + f[0][k][1] * fr[1]);
                n2 += w * (fr[0] * fr[0] + fr[1] * fr[1]);
            }
            p / n2.sqrt()
        };
        assert!(rel(&base[0].e_edges, 0) > 0.9, "TE10 on ŷ sin(πsx)");
        assert!(rel(&base[1].e_edges, 1) > 0.9, "TE20 on ŷ sin(2πsx)");
        assert!(rel(&base[2].e_edges, 2) > 0.9, "TE01 on x̂ sin(πsy)");
    }

    /// A cluster cut by `n_keep` keeps its first canonical direction.
    #[test]
    fn a_cut_cluster_keeps_its_first_canonical_direction() {
        let (mesh, edges, modes) = rect_modes(8, 4, 2.0, 1.0, 3);
        let clusters = [(0, 1), (1, 3)];
        let full = gauge_whitney_modes(&mesh, &edges, &modes[..3], &clusters, 3).unwrap();
        let cut = gauge_whitney_modes(&mesh, &edges, &modes[..3], &clusters, 2).unwrap();
        assert_eq!(cut.len(), 2);
        for (c, f) in cut.iter().zip(&full) {
            assert_eq!(c.e_edges, f.e_edges);
        }
    }

    /// A mode no reference reaches is loud, never silently signed; the
    /// complex sign follows the larger part of the overlap.
    #[test]
    fn unreachable_modes_are_ungaugable_and_complex_signs_follow_the_lead() {
        let quad = GaugeQuadrature {
            weights: vec![0.5, 0.5],
            points: vec![[0.25, 0.5], [0.75, 0.5]],
        };
        let zero = vec![[0.0, 0.0]; 2];
        let err =
            canonical_coefficients(&quad, std::slice::from_ref(&zero), &[(0, 1)], 1).unwrap_err();
        assert!(
            matches!(err, EigenError::UngaugableMode { mode: 0, .. }),
            "{err:?}"
        );
        // An all-zero mode has no sign to pin.
        assert_eq!(reference_sign(&quad, &zero, None, 0).unwrap(), 1.0);
        // ŷ everywhere: positive on the first reference (ŷ sin πsx).
        let up = vec![[0.0, 1.0]; 2];
        let down = vec![[0.0, -1.0]; 2];
        assert_eq!(reference_sign(&quad, &up, None, 0).unwrap(), 1.0);
        assert_eq!(reference_sign(&quad, &down, None, 0).unwrap(), -1.0);
        // Complex: the larger part decides.
        let small = vec![[0.0, 0.1]; 2];
        assert_eq!(reference_sign(&quad, &small, Some(&down), 0).unwrap(), -1.0);
        assert_eq!(reference_sign(&quad, &down, Some(&small), 0).unwrap(), -1.0);
        assert_eq!(reference_sign(&quad, &up, Some(&small), 0).unwrap(), 1.0);
    }
}
