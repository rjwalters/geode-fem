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
//! Inside a cluster of modes with cutoffs `k_c²` within
//! [`DEGENERATE_REL_TOL`] of each other (the TE₁₀ / TE₀₁ pair of a square
//! guide, TE₂₀ / TE₀₁ of a `2 × 1` guide) the solver's basis is arbitrary.
//! The gauge builds the cluster's basis one direction at a time with the
//! same rule: the first reference that reaches the floor and the lead ratio
//! **within the part of the cluster not yet used** fixes the next direction
//! (the combination of the members that overlaps it most, made positive);
//! the next reference is then read in the orthogonal complement. The result
//! is an orthogonal rotation of an M-orthonormal set, so it stays
//! M-orthonormal; each slot keeps its own cutoff. A cluster cut by the
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

/// Relative cutoff gap `(k_c,j+1² − k_c,j²)/k_c,j+1²` at or below which two
/// consecutive modes are one degenerate cluster for the canonical gauge.
pub const DEGENERATE_REL_TOL: f64 = 1e-6;

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
/// `quad`; the modes are M-orthonormal (`∫ e_i·e_j dA = δ_ij`) and
/// sorted by `lambdas` (`k_c²`) ascending. Returns, for each of the first
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
/// Panics if `n_keep > fields.len()` or the lengths are inconsistent.
pub(crate) fn canonical_coefficients(
    quad: &GaugeQuadrature,
    fields: &[Vec<[f64; 2]>],
    lambdas: &[f64],
    n_keep: usize,
) -> Result<Vec<Vec<f64>>, EigenError> {
    let n = fields.len();
    assert!(n_keep <= n, "n_keep exceeds the mode count");
    assert_eq!(lambdas.len(), n, "one cutoff per mode");
    let nq = quad.weights.len();
    assert!(fields.iter().all(|f| f.len() == nq), "field sample count");

    // Clusters: maximal runs of consecutive modes within the tolerance.
    let mut clusters: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for j in 1..=n {
        let split = j == n || {
            let (a, b) = (lambdas[j - 1], lambdas[j]);
            (b - a).abs() > DEGENERATE_REL_TOL * b.abs().max(a.abs())
        };
        if split {
            clusters.push((start, j));
            start = j;
        }
    }

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
        for q in 0..nq {
            let [sx, sy] = quad.points[q];
            let f = reference_field(r, sx, sy);
            let w = quad.weights[q];
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
        let mut used = vec![false; N_REFERENCE_FIELDS];
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

/// Gauge the lowest-order (Whitney) modes of a projected port face (issue
/// #888): `modes` are the M-orthonormal profiles of
/// `solve_waveguide_modes_ungauged`, `λ` ascending (the requested ones,
/// then any further ones of the same solve); returns the first `n_keep`
/// canonical modes ([`canonical_coefficients`]), each with the cutoff of
/// its slot.
///
/// # Errors
///
/// As [`canonical_coefficients`].
pub(crate) fn gauge_whitney_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    modes: &[WaveguideModeProfile],
    n_keep: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let index: std::collections::HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    let (lo, ext) = GaugeQuadrature::bounding_box(mesh);
    let mut quad = GaugeQuadrature::default();
    let mut fields: Vec<Vec<[f64; 2]>> = vec![Vec::new(); modes.len()];
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
            for (mode, f) in modes.iter().zip(fields.iter_mut()) {
                let mut e = [0.0_f64; 2];
                for (l, sh) in local.iter().zip(&shapes) {
                    let v = mode.e_edges[l.0];
                    e[0] += v * sh[0];
                    e[1] += v * sh[1];
                }
                f.push(e);
            }
        }
    }
    let lambdas: Vec<f64> = modes.iter().map(|m| m.lambda).collect();
    let coeffs = canonical_coefficients(&quad, &fields, &lambdas, n_keep)?;
    Ok(coeffs
        .iter()
        .enumerate()
        .map(|(slot, c)| {
            let mut e_edges = vec![0.0_f64; edges.len()];
            for (cj, m) in c.iter().zip(modes) {
                if *cj != 0.0 {
                    e_edges
                        .iter_mut()
                        .zip(&m.e_edges)
                        .for_each(|(x, y)| *x += cj * y);
                }
            }
            WaveguideModeProfile {
                k_c: modes[slot].k_c,
                lambda: modes[slot].lambda,
                e_edges,
            }
        })
        .collect())
}
