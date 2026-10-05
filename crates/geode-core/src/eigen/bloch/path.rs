//! k-path sweeps with overlap-based band tracking (issue #858).
//!
//! # Tracking
//!
//! [`BlochCell::band_structure`] solves every point of a [`KPath`] and
//! connects bands by **eigenvector overlap**, not by sort order. A band
//! that crosses another keeps its identity, which a sorted band diagram
//! loses. Two ingredients make the overlaps meaningful.
//!
//! - **Periodic parts.** A Bloch field is `u e^{−j k·r}` with `u`
//!   periodic. The tracker removes the phase at each edge midpoint, so the
//!   overlaps compare `u(k)` at neighbouring points, which varies
//!   smoothly. The overlap is `|u_aᴴ M u_b|² / (‖u_a‖²_M ‖u_b‖²_M)`.
//! - **Degenerate clusters as subspaces.** Inside a degenerate cluster the
//!   individual eigenvectors are an arbitrary basis. This follows the
//!   #821 coupled-line rule: the tracker rotates every cluster onto the
//!   eigenbasis of its directional derivative matrix along the segment
//!   ([`BlochCell::align_clusters_along`]). Those are the combinations
//!   that continue smoothly along the path, so the per-vector overlaps
//!   with the next point are unambiguous through a splitting or a
//!   crossing.
//!
//! - **Symmetry points.** Where symmetry forces equal slopes in a cluster
//!   (`v_g = 0` at Γ or M), first-order rotation leaves the basis
//!   undetermined. Such a cluster is matched as a **subspace** on either
//!   side: the overlap is the group-summed `Σ |⟨u_s, u_j⟩|²`, and the
//!   slots keep ascending order inside it.
//!
//! The assignment is greedy on the overlap matrix (largest first). It
//! covers `n_bands + n_guard` computed modes per point, so a band can
//! leave the reported window and still be followed. A tracked band whose
//! best overlap drops below [`MIN_CONFIDENT_OVERLAP`] is reported in
//! [`BandStructure::warnings`]: raise `n_guard` or `points_per_segment`.
//! A degenerate cluster at a path **vertex** (where the direction
//! changes) whose slopes are distinct on both sides has no unique
//! continuation across the kink. There the slots keep their order within
//! the cluster, and the vertex is listed in the warnings.

use super::{BlochCell, BlochError, BlochSettings};
use crate::eigen::bloch::hermitian::dotc;

/// Overlap below which a tracking step is reported as uncertain.
pub const MIN_CONFIDENT_OVERLAP: f64 = 0.5;

/// A polyline through k-space.
#[derive(Debug, Clone, PartialEq)]
pub struct KPath {
    vertices: Vec<(String, [f64; 3])>,
    points_per_segment: usize,
}

/// One sample of a [`KPath`].
#[derive(Debug, Clone, PartialEq)]
pub struct KPoint {
    /// The wave vector.
    pub k: [f64; 3],
    /// Arc length along the path from its start.
    pub s: f64,
    /// The vertex label, for a vertex.
    pub label: Option<String>,
    /// Unit direction of the segment arriving here (`None` at the start).
    pub dir_in: Option<[f64; 3]>,
    /// Unit direction of the segment leaving here (`None` at the end).
    pub dir_out: Option<[f64; 3]>,
}

impl KPath {
    /// A path through labelled `vertices`, with `points_per_segment`
    /// samples per segment (the segment's end vertex is the next segment's
    /// first sample).
    ///
    /// # Errors
    ///
    /// [`BlochError::InvalidInput`] for fewer than two vertices, zero
    /// samples per segment, a non-finite vertex or a zero-length segment.
    pub fn new(
        vertices: Vec<(String, [f64; 3])>,
        points_per_segment: usize,
    ) -> Result<Self, BlochError> {
        let bad = |m: String| Err(BlochError::InvalidInput(m));
        if vertices.len() < 2 {
            return bad("a k-path needs at least two vertices".into());
        }
        if points_per_segment == 0 {
            return bad("points_per_segment must be ≥ 1".into());
        }
        for (i, w) in vertices.windows(2).enumerate() {
            if w.iter().any(|(_, k)| k.iter().any(|x| !x.is_finite())) {
                return bad(format!("vertex {i} or {} is not finite", i + 1));
            }
            if dist(w[0].1, w[1].1) == 0.0 {
                return bad(format!(
                    "segment {i} ({} → {}) has zero length",
                    w[0].0, w[1].0
                ));
            }
        }
        Ok(Self {
            vertices,
            points_per_segment,
        })
    }

    /// The square-lattice path Γ–X–M–Γ (`k_z = 0`), lattice constant `a`:
    /// Γ = 0, X = (π/a, 0, 0), M = (π/a, π/a, 0).
    ///
    /// # Panics
    ///
    /// Panics for `a ≤ 0`, a non-finite `a`, or `points_per_segment = 0`.
    pub fn square_lattice(a: f64, points_per_segment: usize) -> Self {
        assert!(a > 0.0 && a.is_finite(), "lattice constant must be > 0");
        let p = std::f64::consts::PI / a;
        Self::new(
            vec![
                ("Γ".into(), [0.0, 0.0, 0.0]),
                ("X".into(), [p, 0.0, 0.0]),
                ("M".into(), [p, p, 0.0]),
                ("Γ".into(), [0.0, 0.0, 0.0]),
            ],
            points_per_segment,
        )
        .expect("the square-lattice path is valid")
    }

    /// The vertices.
    pub fn vertices(&self) -> &[(String, [f64; 3])] {
        &self.vertices
    }

    /// The samples, in path order.
    pub fn points(&self) -> Vec<KPoint> {
        let mut out = Vec::new();
        let mut s0 = 0.0;
        let nseg = self.vertices.len() - 1;
        let unit = |a: [f64; 3], b: [f64; 3]| {
            let l = dist(a, b);
            [(b[0] - a[0]) / l, (b[1] - a[1]) / l, (b[2] - a[2]) / l]
        };
        for i in 0..nseg {
            let (la, a) = &self.vertices[i];
            let b = self.vertices[i + 1].1;
            let len = dist(*a, b);
            let t = unit(*a, b);
            let t_prev = (i > 0).then(|| unit(self.vertices[i - 1].1, *a));
            for j in 0..self.points_per_segment {
                let f = j as f64 / self.points_per_segment as f64;
                out.push(KPoint {
                    k: [
                        a[0] + f * (b[0] - a[0]),
                        a[1] + f * (b[1] - a[1]),
                        a[2] + f * (b[2] - a[2]),
                    ],
                    s: s0 + f * len,
                    label: (j == 0).then(|| la.clone()),
                    dir_in: if j == 0 { t_prev } else { Some(t) },
                    dir_out: Some(t),
                });
            }
            s0 += len;
        }
        let (ll, last) = self.vertices[nseg].clone();
        out.push(KPoint {
            k: last,
            s: s0,
            label: Some(ll),
            dir_in: Some(unit(self.vertices[nseg - 1].1, last)),
            dir_out: None,
        });
        out
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// One overlap-tracked band along a path.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedBand {
    /// `ω = k₀` at each point.
    pub omega: Vec<f64>,
    /// Group velocity along the path, `∂ω/∂s` (units of `c`), from
    /// Hellmann–Feynman: along the outgoing segment, and the incoming one
    /// at the last point. `None` for a static (`ω = 0`) mode.
    pub group_velocity: Vec<Option<f64>>,
    /// The band's index in the sorted spectrum at each point.
    pub sorted_index: Vec<usize>,
    /// Overlap with the previous point (`1` at the first point).
    pub overlap: Vec<f64>,
}

/// Result of [`BlochCell::band_structure`].
#[derive(Debug, Clone, PartialEq)]
pub struct BandStructure {
    /// The path samples.
    pub points: Vec<KPoint>,
    /// The sorted `ω` of every computed mode (`n_bands + n_guard`) at each
    /// point: the conventional band diagram, where band `n` is the `n`-th
    /// lowest.
    pub omega_sorted: Vec<Vec<f64>>,
    /// The `n_bands` overlap-tracked bands. Band `b` starts as the `b`-th
    /// lowest at the first point.
    pub bands: Vec<TrackedBand>,
    /// Tracking and solver warnings.
    pub warnings: Vec<String>,
}

impl BandStructure {
    /// `(min, max)` of the `n`-th **sorted** band over the path.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not a computed band index.
    pub fn sorted_band_range(&self, n: usize) -> (f64, f64) {
        self.omega_sorted
            .iter()
            .map(|w| w[n])
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), w| {
                (lo.min(w), hi.max(w))
            })
    }

    /// The complete gap between sorted bands `n` and `n + 1` along the
    /// path, as `(top of band n, bottom of band n + 1)`, if one opens.
    ///
    /// # Panics
    ///
    /// Panics if `n + 1` is not a computed band index.
    pub fn gap_above(&self, n: usize) -> Option<(f64, f64)> {
        let (_, top) = self.sorted_band_range(n);
        let (bottom, _) = self.sorted_band_range(n + 1);
        (bottom > top).then_some((top, bottom))
    }
}

impl BlochCell {
    /// Solve every point of `path` and track the lowest
    /// `settings.n_bands` bands by overlap (see the [module docs](self)),
    /// computing `n_guard` extra modes per point.
    ///
    /// # Errors
    ///
    /// Any error of [`BlochCell::solve`] at any point (the sweep fails
    /// loudly rather than returning a partial diagram).
    pub fn band_structure(
        &self,
        path: &KPath,
        settings: &BlochSettings,
        n_guard: usize,
    ) -> Result<BandStructure, BlochError> {
        let n_bands = settings.n_bands;
        let solve_settings = BlochSettings {
            n_bands: n_bands + n_guard,
            ..*settings
        };
        let points = path.points();
        let mut omega_sorted = Vec::with_capacity(points.len());
        let mut warnings: Vec<String> = Vec::new();
        let mut slots: Vec<TrackedBand> = Vec::new();
        // Per slot: the previous periodic part, its M-product and norm, and
        // its tracking group at the previous point.
        let mut prev: Vec<(Vec<faer::c64>, Vec<faer::c64>, f64)> = Vec::new();
        let mut prev_group: Vec<usize> = Vec::new();
        for (p, pt) in points.iter().enumerate() {
            let mut modes = self.solve(pt.k, &solve_settings)?;
            // A degenerate cluster cut by the window edge only matters when
            // it reaches into the reported bands (the guards absorb it).
            let top_in_guard = modes.clusters.last().is_some_and(|c| c[0] >= n_bands);
            for w in &modes.warnings {
                if top_in_guard && w.starts_with(super::TOP_CLUSTER_WARNING) {
                    continue;
                }
                let tagged = format!("k = {:?}: {w}", pt.k);
                if !warnings.contains(&tagged) {
                    warnings.push(tagged);
                }
            }
            let n_c = modes.modes.len();
            omega_sorted.push(modes.modes.iter().map(|m| m.omega).collect::<Vec<_>>());
            let v_in = match pt.dir_in {
                Some(t) => self.align_clusters_along(&mut modes, t)?,
                None => vec![None; n_c],
            };
            let parts = self.periodic_parts(&modes);
            let new_group = tracking_groups(&modes, &v_in);
            // Assignment slot → mode.
            let (assign, ov): (Vec<usize>, Vec<f64>) = if p == 0 {
                slots = (0..n_c)
                    .map(|_| TrackedBand {
                        omega: Vec::new(),
                        group_velocity: Vec::new(),
                        sorted_index: Vec::new(),
                        overlap: Vec::new(),
                    })
                    .collect();
                ((0..n_c).collect(), vec![1.0; n_c])
            } else {
                let mut o = vec![vec![0.0; n_c]; n_c];
                for (s, (_, pmu, pn)) in prev.iter().enumerate() {
                    for (j, (u, _, un)) in parts.iter().enumerate() {
                        o[s][j] = dotc(pmu, u).norm_sqr() / (pn * un).max(f64::MIN_POSITIVE);
                    }
                }
                match_groups(&o, &prev_group, &new_group)
            };
            // Outgoing alignment (first point and direction changes).
            let (v_out, parts) = match pt.dir_out {
                Some(t) if pt.dir_in != Some(t) => {
                    let v = self.align_clusters_along(&mut modes, t)?;
                    // A cluster whose slopes are distinct on both sides of a
                    // vertex has no unique continuation across the kink.
                    if p > 0 {
                        let g_out = tracking_groups(&modes, &v);
                        let kinked = modes.clusters.iter().any(|c| {
                            c.len() > 1
                                && !modes.modes[c[0]].is_static
                                && new_group[c[0]] != new_group[c[1]]
                                && g_out[c[0]] != g_out[c[1]]
                        });
                        if kinked {
                            warnings.push(format!(
                                "vertex {} (k = {:?}) has a degenerate cluster with distinct \
                                 slopes on both sides: the continuation of its bands past the \
                                 vertex is not unique (slots keep their order)",
                                pt.label.as_deref().unwrap_or("?"),
                                pt.k
                            ));
                        }
                    }
                    (v, self.periodic_parts(&modes))
                }
                _ => (v_in.clone(), parts),
            };
            let out_group = tracking_groups(&modes, &v_out);
            for (s, slot) in slots.iter_mut().enumerate() {
                let j = assign[s];
                slot.omega.push(modes.modes[j].omega);
                slot.group_velocity.push(if pt.dir_out.is_some() {
                    v_out[j]
                } else {
                    v_in[j]
                });
                slot.sorted_index.push(j);
                slot.overlap.push(ov[s]);
                if s < n_bands && ov[s] < MIN_CONFIDENT_OVERLAP {
                    warnings.push(format!(
                        "band {s}: tracking overlap {:.3} at point {p} (k = {:?}) is below \
                         {MIN_CONFIDENT_OVERLAP}; raise n_guard or points_per_segment",
                        ov[s], pt.k
                    ));
                }
            }
            prev = (0..n_c).map(|s| parts[assign[s]].clone()).collect();
            prev_group = (0..n_c).map(|s| out_group[assign[s]]).collect();
        }
        let bands: Vec<TrackedBand> = slots.into_iter().take(n_bands).collect();
        Ok(BandStructure {
            points,
            omega_sorted,
            bands,
            warnings,
        })
    }
}

/// Slopes closer than this (units of `c`) leave a degenerate cluster's
/// basis undetermined by first-order perturbation theory (e.g. at Γ or M,
/// where symmetry forces `v_g = 0`).
const AMBIGUOUS_SLOPE_TOL: f64 = 1e-3;

/// A tracking-group id per mode. Modes of one degenerate cluster whose
/// directional slopes are (near-)equal share a group: their individual
/// vectors are an arbitrary basis of the cluster subspace, so they are
/// matched as a subspace. Every other mode is its own group.
fn tracking_groups(modes: &super::BlochModes, v: &[Option<f64>]) -> Vec<usize> {
    let n = modes.modes.len();
    let mut g: Vec<usize> = (0..n).collect();
    for cl in &modes.clusters {
        if cl.len() < 2 {
            continue;
        }
        let static_cluster = modes.modes[cl[0]].is_static;
        let slopes: Vec<f64> = cl.iter().filter_map(|&i| v[i]).collect();
        let spread = slopes
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| {
                (lo.min(x), hi.max(x))
            });
        let ambiguous =
            static_cluster || slopes.len() < cl.len() || spread.1 - spread.0 < AMBIGUOUS_SLOPE_TOL;
        if ambiguous {
            for &i in cl {
                g[i] = cl[0];
            }
        }
    }
    g
}

/// Greedy slot → mode assignment on the group-summed overlaps
/// `O'_sj = Σ_{s' ∈ P(s)} Σ_{j' ∈ N(j)} O_{s'j'} / min(|P(s)|, |N(j)|)`
/// (a subspace overlap whenever a side is an ambiguous group), largest
/// first. Inside every group the slots are then re-ordered to match the
/// group's modes in ascending order: within a degenerate subspace there is
/// no individual identity to preserve.
fn match_groups(
    o: &[Vec<f64>],
    prev_group: &[usize],
    new_group: &[usize],
) -> (Vec<usize>, Vec<f64>) {
    let n = o.len();
    let members =
        |g: &[usize], id: usize| -> Vec<usize> { (0..n).filter(|&i| g[i] == id).collect() };
    let mut pairs = Vec::with_capacity(n * n);
    for s in 0..n {
        let ps = members(prev_group, prev_group[s]);
        for j in 0..n {
            let nj = members(new_group, new_group[j]);
            let sum: f64 = ps
                .iter()
                .map(|&a| nj.iter().map(|&b| o[a][b]).sum::<f64>())
                .sum();
            pairs.push((sum / ps.len().min(nj.len()) as f64, s, j));
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut assign = vec![usize::MAX; n];
    let mut ov = vec![0.0; n];
    let mut taken = vec![false; n];
    for (w, s, j) in pairs {
        if assign[s] == usize::MAX && !taken[j] {
            assign[s] = j;
            ov[s] = w.min(1.0);
            taken[j] = true;
        }
    }
    // Canonical order inside groups (previous side, then new side).
    for g in [prev_group, new_group] {
        let mut seen = vec![false; n];
        for start in 0..n {
            let id = g[start];
            if seen[id] {
                continue;
            }
            seen[id] = true;
            let slots: Vec<usize> = if std::ptr::eq(g, prev_group) {
                (0..n).filter(|&s| prev_group[s] == id).collect()
            } else {
                (0..n).filter(|&s| new_group[assign[s]] == id).collect()
            };
            if slots.len() < 2 {
                continue;
            }
            let mut js: Vec<usize> = slots.iter().map(|&s| assign[s]).collect();
            js.sort_unstable();
            let ws: Vec<f64> = slots.iter().map(|&s| ov[s]).collect();
            let wmin = ws.iter().copied().fold(1.0, f64::min);
            for (&s, &j) in slots.iter().zip(&js) {
                assign[s] = j;
                ov[s] = wmin;
            }
        }
    }
    (assign, ov)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_lattice_path_samples_and_directions() {
        let p = KPath::square_lattice(1.0, 2).points();
        assert_eq!(p.len(), 7);
        let pi = std::f64::consts::PI;
        assert_eq!(p[0].label.as_deref(), Some("Γ"));
        assert_eq!(p[2].label.as_deref(), Some("X"));
        assert_eq!(p[4].label.as_deref(), Some("M"));
        assert_eq!(p[6].label.as_deref(), Some("Γ"));
        assert_eq!(p[1].k, [0.5 * pi, 0.0, 0.0]);
        assert_eq!(p[0].dir_in, None);
        assert_eq!(p[6].dir_out, None);
        assert_eq!(p[2].dir_in, Some([1.0, 0.0, 0.0]));
        assert_eq!(p[2].dir_out, Some([0.0, 1.0, 0.0]));
        let total = 2.0 * pi + 2f64.sqrt() * pi;
        assert!((p[6].s - total).abs() < 1e-12);
        assert!(KPath::new(vec![("A".into(), [0.0; 3])], 4).is_err());
        assert!(KPath::new(vec![("A".into(), [0.0; 3]), ("B".into(), [0.0; 3])], 4).is_err());
        assert!(
            KPath::new(
                vec![("A".into(), [0.0; 3]), ("B".into(), [1.0, 0.0, 0.0])],
                0
            )
            .is_err()
        );
    }

    #[test]
    fn group_matching_follows_overlaps_and_treats_groups_as_subspaces() {
        // Singletons: a swap is followed.
        let o = vec![
            vec![0.1, 0.9, 0.0],
            vec![0.9, 0.1, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        let (a, ov) = match_groups(&o, &[0, 1, 2], &[0, 1, 2]);
        assert_eq!(a, vec![1, 0, 2]);
        assert!(ov.iter().all(|&x| x >= 0.9));
        // New modes 0, 1 form an ambiguous group: slot 0 and slot 1 each
        // overlap half with both; as a subspace they match fully, in order.
        let o = vec![
            vec![0.5, 0.5, 0.0],
            vec![0.5, 0.5, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        let (a, ov) = match_groups(&o, &[0, 1, 2], &[0, 0, 2]);
        assert_eq!(a, vec![0, 1, 2]);
        assert!(ov.iter().all(|&x| (x - 1.0).abs() < 1e-12), "{ov:?}");
    }
}
