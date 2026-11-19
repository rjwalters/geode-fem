//! Box-averaged peak-field observable at conductor corners (issue #1480,
//! slice A of #1477).
//!
//! A pointwise maximum of the P1 electrostatic field at a convex conductor
//! corner is mesh-dependent: the field is singular there (`|E| ~ r^(-1/3)`,
//! see [`crate::postproc::sample`]), so the per-tet maximum grows as the
//! mesh is refined and is not a reportable quantity. The observable here
//! reports instead the maximum — over candidate window centres — of the
//! field *averaged over a small box*, which converges under refinement,
//! plus the location of that maximum and an enhancement factor against a
//! caller-supplied reference field `E_ref`.
//!
//! Candidate window centres are the centroids of the region's tets, scanned
//! in ascending tet index; all windows are batched through one
//! [`sample_cell_average`] call (serial, exact `tet ∩ box` volume
//! weighting). The result is therefore deterministic and independent of the
//! thread count, and ties resolve to the lowest candidate index.
//!
//! The pointwise per-tet maximum over the region is reported alongside for
//! diagnostics, labelled mesh-dependent: compare it across a refinement
//! series with [`pointwise_non_convergent`]. Trend fitting is out of scope
//! here (issue #1481 owns the estimator/DWR slice).
//!
//! [`conduction_window_peaks`] applies the same windowing to a conduction
//! solution's current density and Joule density (issue #1496): the
//! re-entrant corner of an L-shaped conductor makes the per-element `|J|`
//! singular the same way, and each of the four averages (`|<J>|`, `<|J|>`,
//! `sqrt(<|J|²>)`, `<σ|∇φ|²>`) tracks its own winning window over the
//! conducting tets, normalized by the conductor-intersection volume.

use crate::mesh::{TaggedTetMesh, TetMesh};
use crate::postproc::sample::{
    Aabb, IncludedCellAverage, sample_cell_average, sample_included_cell_average,
};

/// Region over which the observable is evaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Region {
    /// Axis-aligned box; a tet belongs to the region when its centroid lies
    /// inside the box (inclusive bounds).
    Box(Aabb),
    /// Tets carrying this 3-D physical-group tag (resolve a group *name*
    /// through [`TaggedTetMesh::physical_group_tag`] with `dim = 3`).
    /// Requires per-tet tags: pass them to [`peak_field`] or use
    /// [`peak_field_tagged`].
    Tag(i32),
}

/// Errors produced by the peak-field observable.
#[derive(Debug, thiserror::Error)]
pub enum PeakFieldError {
    #[error("region contains no tets")]
    EmptyRegion,
    #[error("tag region requires per-tet physical tags (tet_tags or peak_field_tagged)")]
    MissingTags,
    #[error("window edge must be positive and finite, got {0}")]
    BadWindowEdge(f64),
    #[error("reference field E_ref must be positive and finite, got {0}")]
    BadReference(f64),
}

/// Peak-field observable over a region (issue #1480).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakField {
    /// Maximum, over candidate windows, of the volume-weighted mean `|E|`
    /// (box of edge [`peak_field`]'s `window_edge`, centred at each region
    /// tet's centroid). This is the convergent, reportable value.
    pub value: f64,
    /// Centre of the window that produced [`PeakField::value`].
    pub location: [f64; 3],
    /// [`PeakField::value`] `/ E_ref` for the caller-supplied reference
    /// field.
    pub enhancement: f64,
    /// Pointwise per-tet maximum `|E|` over the region.
    ///
    /// **Mesh-dependent**: at a convex corner this grows under refinement
    /// (the field is singular), so it is reported for diagnostics only —
    /// never as a reportable quantity. Check a refinement series with
    /// [`pointwise_non_convergent`].
    pub pointwise_max: f64,
    /// Under-sampling warning: `true` when `window_edge` is below the
    /// largest element edge among the tets whose bounding box intersects
    /// the winning window — the average then spans fewer than the local
    /// elements and does not resolve the local field.
    pub undersampled: bool,
    /// The winning physical window: the box of edge `window_edge` centred
    /// on [`PeakField::location`] (exactly the box the scan averaged over,
    /// so re-weighting tets against it reproduces [`PeakField::value`]).
    /// On an exact tie this is the lowest-index candidate's box — the
    /// deterministic branch the goal estimator (#1766) linearizes.
    pub window: Aabb,
}

/// Region membership as ascending tet indices.
///
/// [`Region::Box`] selects tets whose centroid lies inside the box;
/// [`Region::Tag`] selects tets carrying the given 3-D physical tag, which
/// requires `tet_tags` (parallel to `mesh.tets`).
///
/// # Panics
///
/// Panics if `Region::Tag` is used with a `tet_tags` slice whose length is
/// not `mesh.n_tets()`.
pub fn region_tet_indices(
    mesh: &TetMesh,
    tet_tags: Option<&[i32]>,
    region: &Region,
) -> Result<Vec<u32>, PeakFieldError> {
    match region {
        Region::Box(box_) => Ok(mesh
            .tets
            .iter()
            .enumerate()
            .filter_map(|(t, tet)| {
                let c = tet_centroid(mesh, tet);
                ((0..3).all(|d| c[d] >= box_.min[d] && c[d] <= box_.max[d])).then_some(t as u32)
            })
            .collect()),
        Region::Tag(tag) => {
            let tags = tet_tags.ok_or(PeakFieldError::MissingTags)?;
            assert_eq!(
                tags.len(),
                mesh.n_tets(),
                "tet_tags length {} != tet count {}",
                tags.len(),
                mesh.n_tets()
            );
            Ok(tags
                .iter()
                .enumerate()
                .filter_map(|(t, &tg)| (tg == *tag).then_some(t as u32))
                .collect())
        }
    }
}

/// Whether a pointwise-maximum refinement series is mesh-dependent.
///
/// `series` holds the pointwise maxima over a refinement series in
/// coarse-to-fine order. Returns `true` when the value **grows** from the
/// first to the last entry — the signature of a singular, mesh-dependent
/// quantity (no trend detection; per issue #1480 the series is simply
/// labelled). Fewer than two entries, or a non-growing series, is `false`.
pub fn pointwise_non_convergent(series: &[f64]) -> bool {
    match (series.first(), series.last()) {
        (Some(&first), Some(&last)) => last > first,
        _ => false,
    }
}

/// Maximum `|E|` over candidate windows in `region`, with location and
/// enhancement factor (issue #1480).
///
/// Candidate window centres are the centroids of the region's tets; each
/// window is a cube of edge `window_edge` centred there, and all windows
/// are averaged in one batched [`sample_cell_average`] call. The winning
/// window is the one with the largest volume-weighted mean magnitude
/// [`sample_cell_average`] reports for the per-tet field `field_per_tet`
/// (typically [`recover_e_field`](crate::assembly::electrostatic::recover_e_field)
/// applied to a solved potential).
///
/// # Errors
///
/// - [`PeakFieldError::EmptyRegion`] if the region selects no tets (e.g. a
///   box outside the mesh, or an unused tag).
/// - [`PeakFieldError::MissingTags`] if `region` is [`Region::Tag`] but
///   `tet_tags` is `None`.
/// - [`PeakFieldError::BadWindowEdge`] / [`PeakFieldError::BadReference`]
///   for non-positive or non-finite `window_edge` / `e_ref`.
///
/// # Panics
///
/// Panics if `field_per_tet.len() != mesh.n_tets()` (delegated to
/// [`sample_cell_average`]), or if `Region::Tag` is used with a
/// `tet_tags` slice of the wrong length.
pub fn peak_field(
    mesh: &TetMesh,
    tet_tags: Option<&[i32]>,
    field_per_tet: &[[f64; 3]],
    region: &Region,
    window_edge: f64,
    e_ref: f64,
) -> Result<PeakField, PeakFieldError> {
    if window_edge <= 0.0 || !window_edge.is_finite() {
        return Err(PeakFieldError::BadWindowEdge(window_edge));
    }
    if e_ref <= 0.0 || !e_ref.is_finite() {
        return Err(PeakFieldError::BadReference(e_ref));
    }
    let region_tets = region_tet_indices(mesh, tet_tags, region)?;
    if region_tets.is_empty() {
        return Err(PeakFieldError::EmptyRegion);
    }

    let half = window_edge / 2.0;
    let windows: Vec<Aabb> = region_tets
        .iter()
        .map(|&t| {
            let c = tet_centroid(mesh, &mesh.tets[t as usize]);
            Aabb {
                min: [c[0] - half, c[1] - half, c[2] - half],
                max: [c[0] + half, c[1] + half, c[2] + half],
            }
        })
        .collect();
    let avgs = sample_cell_average(mesh, field_per_tet, &windows);

    // Ascending candidate order; strict `>` keeps the lowest index on ties.
    let mut best: Option<usize> = None;
    for (i, a) in avgs.iter().enumerate() {
        if a.volume_covered <= 0.0 {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => a.mean_magnitude > avgs[b].mean_magnitude,
        };
        if better {
            best = Some(i);
        }
    }
    let best_i = best.ok_or(PeakFieldError::EmptyRegion)?;
    let location = {
        let w = &windows[best_i];
        [
            (w.min[0] + w.max[0]) / 2.0,
            (w.min[1] + w.max[1]) / 2.0,
            (w.min[2] + w.max[2]) / 2.0,
        ]
    };
    let value = avgs[best_i].mean_magnitude;

    let pointwise_max = region_tets
        .iter()
        .map(|&t| norm2(field_per_tet[t as usize]))
        .fold(f64::NEG_INFINITY, f64::max);

    let undersampled = window_edge < max_element_edge_intersecting(mesh, &windows[best_i]);

    Ok(PeakField {
        value,
        location,
        enhancement: value / e_ref,
        pointwise_max,
        undersampled,
        window: windows[best_i],
    })
}

/// [`peak_field`] over a [`TaggedTetMesh`]: the per-tet physical tags come
/// from the tagged mesh, so [`Region::Tag`] works without passing them.
pub fn peak_field_tagged(
    tagged: &TaggedTetMesh,
    field_per_tet: &[[f64; 3]],
    region: &Region,
    window_edge: f64,
    e_ref: f64,
) -> Result<PeakField, PeakFieldError> {
    peak_field(
        &tagged.mesh,
        Some(&tagged.tet_physical_tags),
        field_per_tet,
        region,
        window_edge,
        e_ref,
    )
}

// ------------------------------------------------------- conduction windows

/// One metric's winning window of the windowed conduction observable
/// ([`conduction_window_peaks`], issue #1496).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductionWindowPeak {
    /// Peak value of the metric over the candidate windows (the same
    /// mesh-unit scaling as the per-tet fields passed in).
    pub value: f64,
    /// Centre of the winning window.
    pub location: [f64; 3],
    /// Conducting overlap of the winning window divided by its full box
    /// volume (conductor-intersection normalization): `1` when the window
    /// sits entirely inside conducting tets, smaller when it spills past
    /// the conductor surface or into insulating tets.
    pub coverage_fraction: f64,
    /// Under-sampling warning: `true` when `window_edge` is below the
    /// largest element edge among the tets whose bounding box intersects
    /// the winning window (the issue #1480 bounding-box rule) — the
    /// average then spans fewer than the local elements.
    pub undersampled: bool,
}

/// Windowed peak current-density / Joule-density observable for a
/// conduction solve (issue #1496).
///
/// Per-element maxima of `|J|` and the Joule density are mesh-dependent at
/// a re-entrant corner (`|J| ~ r^(-1/3)`, integrable but unbounded); a
/// *windowed average over a fixed physical box* converges under uniform
/// refinement and is the reportable quantity. Each metric below tracks its
/// own winning window over the same candidate scan (windows of edge
/// `window_edge` centred on the conducting tets, ascending tet index,
/// lowest index on ties — the same rules as [`peak_field`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductionWindowPeaks {
    /// Magnitude of the volume-averaged current-density vector, `|<J>|`.
    pub vector_mean: ConductionWindowPeak,
    /// Volume-averaged current-density magnitude, `<|J|>`.
    pub magnitude_mean: ConductionWindowPeak,
    /// Root-mean-square current-density magnitude, `sqrt(<|J|²>)`.
    pub magnitude_rms: ConductionWindowPeak,
    /// Volume-averaged Joule density, `<σ|∇φ|²>`.
    pub joule_mean: ConductionWindowPeak,
    /// Pointwise per-tet maximum `|J|` over the conducting tets.
    ///
    /// **Mesh-dependent**: at a re-entrant corner this grows under
    /// refinement, so it is reported for diagnostics only — never as a
    /// reportable quantity.
    pub pointwise_max_j: f64,
    /// Pointwise per-tet maximum Joule density over the conducting tets.
    /// **Mesh-dependent** in the same way as [`ConductionWindowPeaks::pointwise_max_j`].
    pub pointwise_max_joule: f64,
}

/// Windowed peak current-density / Joule-density averages for a conduction
/// solution (issue #1496).
///
/// Candidate window centres are the centroids of the tets with
/// `conducting[t] == true` (positive conductivity); each window is a cube
/// of edge `window_edge` centred there. All windows are averaged in one
/// batched [`sample_included_cell_average`] call with the *same*
/// `conducting` mask, so both the numerators and the denominator of every
/// average count conducting-tet overlaps only (conductor-intersection
/// normalization). Fields are per-tet in the solve's own scaling (pass
/// `j_per_tet = sol.j`, `joule_per_tet = sol.joule_density`); the returned
/// values carry that same scaling, so a caller reporting SI divides by its
/// length unit once (`J`) or squared (Joule density).
///
/// # Errors
///
/// - [`PeakFieldError::EmptyRegion`] if no tet is conducting, or no
///   window overlaps a conducting tet.
/// - [`PeakFieldError::BadWindowEdge`] for a non-positive or non-finite
///   `window_edge`.
///
/// # Panics
///
/// Panics if `j_per_tet`, `joule_per_tet` or `conducting` lengths differ
/// from `mesh.n_tets()` (delegated to [`sample_included_cell_average`]).
pub fn conduction_window_peaks(
    mesh: &TetMesh,
    j_per_tet: &[[f64; 3]],
    joule_per_tet: &[f64],
    conducting: &[bool],
    window_edge: f64,
) -> Result<ConductionWindowPeaks, PeakFieldError> {
    if window_edge <= 0.0 || !window_edge.is_finite() {
        return Err(PeakFieldError::BadWindowEdge(window_edge));
    }
    let region_tets: Vec<u32> = (0..mesh.n_tets())
        .filter(|&t| conducting[t])
        .map(|t| t as u32)
        .collect();
    if region_tets.is_empty() {
        return Err(PeakFieldError::EmptyRegion);
    }

    let half = window_edge / 2.0;
    let windows: Vec<Aabb> = region_tets
        .iter()
        .map(|&t| {
            let c = tet_centroid(mesh, &mesh.tets[t as usize]);
            Aabb {
                min: [c[0] - half, c[1] - half, c[2] - half],
                max: [c[0] + half, c[1] + half, c[2] + half],
            }
        })
        .collect();
    let avgs = sample_included_cell_average(mesh, j_per_tet, joule_per_tet, conducting, &windows);

    // The four metrics on each candidate; strict `>` in ascending candidate
    // order keeps the lowest index on ties (one winner per metric).
    let metric = |k: usize, a: &IncludedCellAverage| -> f64 {
        match k {
            0 => norm2(a.mean),
            1 => a.mean_magnitude,
            2 => a.mean_square_magnitude.sqrt(),
            _ => a.mean_scalar,
        }
    };
    let mut best: [Option<usize>; 4] = [None; 4];
    for (i, a) in avgs.iter().enumerate() {
        if a.volume_included <= 0.0 {
            continue;
        }
        for (k, b) in best.iter_mut().enumerate() {
            let better = match *b {
                None => true,
                Some(b) => metric(k, a) > metric(k, &avgs[b]),
            };
            if better {
                *b = Some(i);
            }
        }
    }
    if best.iter().any(Option::is_none) {
        return Err(PeakFieldError::EmptyRegion);
    }
    let window_volume = window_edge * window_edge * window_edge;
    let winner = |k: usize| -> ConductionWindowPeak {
        let i = best[k].expect("checked above: every metric found a covered candidate");
        let w = &windows[i];
        ConductionWindowPeak {
            value: metric(k, &avgs[i]),
            location: [
                (w.min[0] + w.max[0]) / 2.0,
                (w.min[1] + w.max[1]) / 2.0,
                (w.min[2] + w.max[2]) / 2.0,
            ],
            coverage_fraction: avgs[i].volume_included / window_volume,
            undersampled: window_edge < max_element_edge_intersecting(mesh, w),
        }
    };

    let pointwise_max_j = region_tets
        .iter()
        .map(|&t| norm2(j_per_tet[t as usize]))
        .fold(f64::NEG_INFINITY, f64::max);
    let pointwise_max_joule = region_tets
        .iter()
        .map(|&t| joule_per_tet[t as usize])
        .fold(f64::NEG_INFINITY, f64::max);

    Ok(ConductionWindowPeaks {
        vector_mean: winner(0),
        magnitude_mean: winner(1),
        magnitude_rms: winner(2),
        joule_mean: winner(3),
        pointwise_max_j,
        pointwise_max_joule,
    })
}

// ----------------------------------------------------------------- helpers

type V3 = [f64; 3];

fn norm2(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn tet_centroid(mesh: &TetMesh, tet: &[u32; 4]) -> V3 {
    let mut c = [0.0; 3];
    for &v in tet {
        let p = mesh.nodes[v as usize];
        for d in 0..3 {
            c[d] += p[d];
        }
    }
    [c[0] / 4.0, c[1] / 4.0, c[2] / 4.0]
}

fn tet_bbox(mesh: &TetMesh, tet: &[u32; 4]) -> (V3, V3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &v in tet {
        let p = mesh.nodes[v as usize];
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    (lo, hi)
}

fn tet_max_edge(mesh: &TetMesh, tet: &[u32; 4]) -> f64 {
    let mut max = 0.0_f64;
    for i in 0..4 {
        for j in (i + 1)..4 {
            let a = mesh.nodes[tet[i] as usize];
            let b = mesh.nodes[tet[j] as usize];
            let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
            max = max.max(norm2(d));
        }
    }
    max
}

/// Largest element edge among the tets whose bounding box intersects
/// `window` (the issue's window-size vs. element-size warning input).
fn max_element_edge_intersecting(mesh: &TetMesh, window: &Aabb) -> f64 {
    mesh.tets
        .iter()
        .filter_map(|tet| {
            let (lo, hi) = tet_bbox(mesh, tet);
            let intersects = (0..3).all(|d| hi[d] > window.min[d] && lo[d] < window.max[d]);
            intersects.then(|| tet_max_edge(mesh, tet))
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    /// Uniform field: `E = [A_x, A_y, A_z]` everywhere, so every window
    /// averages to `|A|` regardless of the mesh.
    fn uniform_field(mesh: &TetMesh, a: V3) -> Vec<V3> {
        vec![a; mesh.n_tets()]
    }

    #[test]
    fn uniform_field_value_location_and_enhancement() {
        let mesh = cube_tet_mesh(3, 1.0);
        let a = [2.0, -1.0, 0.5];
        let e = uniform_field(&mesh, a);
        let expected = norm2(a);
        let region = Region::Box(Aabb {
            min: [0.0; 3],
            max: [0.5; 3],
        });
        let r = peak_field(&mesh, None, &e, &region, 0.1, 2.5).expect("ok");
        assert!((r.value - expected).abs() < 1e-12, "{}", r.value);
        assert!((r.enhancement - expected / 2.5).abs() < 1e-12);
        // A uniform field has no preferred location (per-window means are
        // equal up to float round-off); the winner must be a candidate
        // centroid inside the region.
        assert!(
            (0..3).all(|d| r.location[d] >= 0.0 && r.location[d] <= 0.5),
            "location {:?} outside region",
            r.location
        );
        // Window (0.1) < element edge (1/3 * sqrt(2)): under-sampled.
        assert!(r.undersampled, "0.1 < edge {}", 1.0 / 3.0 * 2.0f64.sqrt());
        assert!((r.pointwise_max - expected).abs() < 1e-12);
    }

    #[test]
    fn under_sampled_warning_clears_when_window_exceeds_element() {
        let mesh = cube_tet_mesh(6, 1.0); // element edge ≈ sqrt(1/6²+1/6²)
        let e = uniform_field(&mesh, [1.0, 0.0, 0.0]);
        let region = Region::Box(Aabb {
            min: [0.0; 3],
            max: [0.5; 3],
        });
        let big = peak_field(&mesh, None, &e, &region, 0.5, 1.0).expect("ok");
        assert!(!big.undersampled);
        let small = peak_field(&mesh, None, &e, &region, 0.01, 1.0).expect("ok");
        assert!(small.undersampled);
    }

    #[test]
    fn peak_tracks_a_localised_bump() {
        // One tet near the corner carries a huge field; the window average
        // must exceed the uniform background and sit at that corner.
        let mesh = cube_tet_mesh(4, 1.0);
        let mut e = vec![[1.0, 0.0, 0.0]; mesh.n_tets()];
        e[0] = [100.0, 0.0, 0.0];
        let region = Region::Box(Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        });
        let r = peak_field(&mesh, None, &e, &region, 0.15, 1.0).expect("ok");
        assert!(r.value > 1.0, "bump must raise the average: {}", r.value);
        assert!(r.location[0] < 0.2 && r.location[1] < 0.2 && r.location[2] < 0.2);
        assert!((r.pointwise_max - 100.0).abs() < 1e-12);
    }

    #[test]
    fn tag_region_and_tagged_wrapper() {
        let mesh = cube_tet_mesh(2, 1.0);
        // Tag only the first half of the tets; the rest carry no group.
        let tags = (0..mesh.n_tets())
            .map(|t| if t < mesh.n_tets() / 2 { 7 } else { 0 })
            .collect::<Vec<_>>();
        let e = uniform_field(&mesh, [1.0, 2.0, 2.0]);
        // An unused tag: no tet carries 9.
        let unused = Region::Tag(9);
        assert!(
            peak_field(&mesh, Some(&tags), &e, &unused, 0.2, 1.0).is_err(),
            "tag 9 has no tets here"
        );
        let tagged = TaggedTetMesh {
            mesh: mesh.clone(),
            tet_physical_tags: tags,
            boundary_triangles: Vec::new(),
            triangle_physical_tags: Vec::new(),
        };
        let r = peak_field_tagged(&tagged, &e, &Region::Tag(7), 0.3, 3.0).expect("ok");
        assert!((r.value - 3.0).abs() < 1e-12);
        assert!((r.enhancement - 1.0).abs() < 1e-12);
    }

    #[test]
    fn missing_tags_is_an_error() {
        let mesh = cube_tet_mesh(2, 1.0);
        let e = uniform_field(&mesh, [1.0, 0.0, 0.0]);
        let err = peak_field(&mesh, None, &e, &Region::Tag(3), 0.2, 1.0).unwrap_err();
        assert!(matches!(err, PeakFieldError::MissingTags));
    }

    #[test]
    fn empty_region_errors() {
        let mesh = cube_tet_mesh(2, 1.0);
        let e = uniform_field(&mesh, [1.0, 0.0, 0.0]);
        // Box entirely outside the mesh.
        let outside = Region::Box(Aabb {
            min: [5.0; 3],
            max: [6.0; 3],
        });
        assert!(matches!(
            peak_field(&mesh, None, &e, &outside, 0.2, 1.0),
            Err(PeakFieldError::EmptyRegion)
        ));
    }

    #[test]
    fn window_larger_than_region_is_fine() {
        // Region smaller than the window: windows spill past the region but
        // still average the mesh around the centroids. (Centroids of the
        // n = 4 cube mesh sit at multiples of 1/16, so a 0.25-wide box
        // around the centre holds candidates while a 0.5 window dwarfs it.)
        let mesh = cube_tet_mesh(4, 1.0);
        let e = uniform_field(&mesh, [1.0, 0.0, 0.0]);
        let region = Region::Box(Aabb {
            min: [0.25; 3],
            max: [0.5; 3],
        });
        let r = peak_field(&mesh, None, &e, &region, 0.5, 1.0).expect("ok");
        assert!((r.value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn bad_window_and_reference_errors() {
        let mesh = cube_tet_mesh(2, 1.0);
        let e = uniform_field(&mesh, [1.0, 0.0, 0.0]);
        let region = Region::Box(Aabb {
            min: [0.0; 3],
            max: [0.5; 3],
        });
        for edge in [0.0, -1.0, f64::NAN] {
            assert!(matches!(
                peak_field(&mesh, None, &e, &region, edge, 1.0),
                Err(PeakFieldError::BadWindowEdge(_))
            ));
        }
        for r in [0.0, -1.0, f64::NAN] {
            assert!(matches!(
                peak_field(&mesh, None, &e, &region, 0.2, r),
                Err(PeakFieldError::BadReference(_))
            ));
        }
    }

    #[test]
    fn deterministic_bitwise_across_calls() {
        let mesh = cube_tet_mesh(4, 1.0);
        let mut e = vec![[1.0, 0.0, 0.0]; mesh.n_tets()];
        for (i, v) in e.iter_mut().enumerate().take(20) {
            v[0] = 10.0 + i as f64;
        }
        let region = Region::Box(Aabb {
            min: [0.0; 3],
            max: [0.5; 3],
        });
        let a = peak_field(&mesh, None, &e, &region, 0.15, 1.0).expect("ok");
        let b = peak_field(&mesh, None, &e, &region, 0.15, 1.0).expect("ok");
        assert_eq!(a, b);
    }

    #[test]
    fn series_label() {
        assert!(pointwise_non_convergent(&[10.0, 12.0, 15.0]));
        assert!(!pointwise_non_convergent(&[15.0, 15.0, 15.0]));
        assert!(!pointwise_non_convergent(&[15.0]));
        assert!(!pointwise_non_convergent(&[]));
    }

    // ------------------------------------------------ conduction window tests

    /// Mesh of isolated right tets at the given offsets (each
    /// `[o, o+(1,0,0), o+(0,1,0), o+(0,0,1)]`, volume 1/6, disjoint).
    fn tets_at(offsets: &[[f64; 3]]) -> TetMesh {
        let mut nodes = Vec::new();
        let mut tets = Vec::new();
        for o in offsets {
            let base = nodes.len() as u32;
            nodes.push(*o);
            nodes.push([o[0] + 1.0, o[1], o[2]]);
            nodes.push([o[0], o[1] + 1.0, o[2]]);
            nodes.push([o[0], o[1], o[2] + 1.0]);
            tets.push([base, base + 1, base + 2, base + 3]);
        }
        TetMesh {
            nodes,
            tets,
            physical_groups: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn conduction_window_hand_computed_equal_overlap() {
        // Two conducting tets of equal volume (1/6) with J = +1 and −3
        // along x and Joule densities 1 and 6 (σ = 1 and 1.5 under
        // |J|² = 1 and 9), a third insulating tet in between. A window
        // large enough to contain both conducting tets fully gives equal
        // overlap weights, so (issue #1496): |<J>| = 1, <|J|> = 2,
        // RMS = √5, <q> = 3.5. The insulating tet's fields (huge) must
        // not leak into any average, and its overlap must not count in
        // the coverage either.
        let mesh = tets_at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 0.0, 0.0]]);
        let j = vec![[1.0, 0.0, 0.0], [-3.0, 0.0, 0.0], [0.0, 0.0, 888.0]];
        let joule = vec![1.0, 6.0, 999.0];
        let conducting = [true, true, false];
        // Edge 22: both windows (centred at x = 0.25 and x = 10.25)
        // contain both conducting tets fully; the insulating tet at
        // x ∈ [5, 6] too, but the mask excludes it.
        let r = conduction_window_peaks(&mesh, &j, &joule, &conducting, 22.0).expect("ok");
        assert!(
            (r.vector_mean.value - 1.0).abs() < 1e-12,
            "{}",
            r.vector_mean.value
        );
        assert!((r.magnitude_mean.value - 2.0).abs() < 1e-12);
        assert!((r.magnitude_rms.value - 5.0f64.sqrt()).abs() < 1e-12);
        assert!((r.joule_mean.value - 3.5).abs() < 1e-12);
        // Identical windows (same two tets, same accumulation order) tie;
        // the lowest index wins: location = first tet's centroid.
        let expected_cov = (1.0 / 6.0 + 1.0 / 6.0) / (22.0 * 22.0 * 22.0);
        for w in [
            &r.vector_mean,
            &r.magnitude_mean,
            &r.magnitude_rms,
            &r.joule_mean,
        ] {
            assert!((w.location[0] - 0.25).abs() < 1e-12, "{w:?}");
            assert!(
                (w.coverage_fraction - expected_cov).abs() < 1e-18,
                "{w:?} vs {expected_cov}"
            );
            assert!(!w.undersampled, "window 22 ≫ element edge √3");
        }
        assert!((r.pointwise_max_j - 3.0).abs() < 1e-12);
        assert!((r.pointwise_max_joule - 6.0).abs() < 1e-12);
    }

    #[test]
    fn conduction_window_metrics_pick_different_windows() {
        // Three separate windows: A over one tet with J = +10; B over two
        // equal conducting tets with J = ±12 (cancelling vector mean,
        // large magnitude metrics); C over one tet with a huge Joule
        // density and vanishing J. |<J>| must win at A, <|J|> / RMS at B,
        // <q> at C — different metrics, different winning boxes.
        let nodes = vec![
            [5.0, 0.0, 0.0], // A
            [6.0, 0.0, 0.0],
            [5.0, 1.0, 0.0],
            [5.0, 0.0, 1.0],
            [0.0, 0.0, 0.0], // B1 at the origin
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0], // B2: [(1,0,0),(0,1,0),(1,1,0),(1,0,1)], vol 1/6
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 1.0],
            [9.0, 0.0, 0.0], // C
            [10.0, 0.0, 0.0],
            [9.0, 1.0, 0.0],
            [9.0, 0.0, 1.0],
        ];
        let tets: Vec<[u32; 4]> = vec![
            [0, 1, 2, 3],     // A at (5,0,0)
            [4, 5, 6, 7],     // B1 at the origin
            [8, 9, 10, 11],   // B2
            [12, 13, 14, 15], // C at (9,0,0)
        ];
        let mesh = TetMesh {
            nodes,
            tets,
            physical_groups: std::collections::BTreeMap::new(),
        };
        let j = vec![
            [10.0, 0.0, 0.0],
            [12.0, 0.0, 0.0],
            [-12.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let joule = vec![10.0, 100.0, 100.0, 200.0];
        let conducting = [true; 4];
        // Edge 2: B's window (centred at B1's centroid (0.25, 0.25, 0.25))
        // covers B1 and B2 fully (x ≤ 1.25) and excludes A and C; A's and
        // C's windows each contain only their own tet.
        let r = conduction_window_peaks(&mesh, &j, &joule, &conducting, 2.0).expect("ok");
        // |<J>|: A 10, B 0, C 0 → A.
        assert!(
            (r.vector_mean.value - 10.0).abs() < 1e-12,
            "{:?}",
            r.vector_mean
        );
        assert!((r.vector_mean.location[0] - 5.25).abs() < 1e-12);
        // <|J|> and RMS: B 12, A 10, C 0 → B.
        assert!(
            (r.magnitude_mean.value - 12.0).abs() < 1e-12,
            "{:?}",
            r.magnitude_mean
        );
        assert!(
            (r.magnitude_rms.value - 12.0).abs() < 1e-12,
            "{:?}",
            r.magnitude_rms
        );
        assert!((r.magnitude_mean.location[0] - 0.25).abs() < 1e-12);
        assert!((r.magnitude_rms.location[0] - 0.25).abs() < 1e-12);
        // <q>: C 200, B 100, A 10 → C.
        assert!(
            (r.joule_mean.value - 200.0).abs() < 1e-12,
            "{:?}",
            r.joule_mean
        );
        assert!((r.joule_mean.location[0] - 9.25).abs() < 1e-12);
        // Coverage: each window fully contains exactly its own conducting
        // tet(s) — A and C hold one tet of volume 1/6 in a 2³ window, B two.
        let sixth = 1.0 / 6.0;
        assert!((r.vector_mean.coverage_fraction - sixth / 8.0).abs() < 1e-15);
        assert!((r.joule_mean.coverage_fraction - sixth / 8.0).abs() < 1e-15);
        assert!(
            (r.magnitude_mean.coverage_fraction - 2.0 * sixth / 8.0).abs() < 1e-15,
            "{:?}",
            r.magnitude_mean
        );
        for w in [
            &r.vector_mean,
            &r.magnitude_mean,
            &r.magnitude_rms,
            &r.joule_mean,
        ] {
            assert!(!w.undersampled);
        }
        assert!((r.pointwise_max_j - 12.0).abs() < 1e-12);
        assert!((r.pointwise_max_joule - 200.0).abs() < 1e-12);
    }

    #[test]
    fn conduction_window_undersampled_warning() {
        let mesh = cube_tet_mesh(2, 1.0); // max element edge 0.5·√3 ≈ 0.866
        let j = vec![[1.0, 0.0, 0.0]; mesh.n_tets()];
        let joule = vec![2.0; mesh.n_tets()];
        let conducting = vec![true; mesh.n_tets()];
        let small = conduction_window_peaks(&mesh, &j, &joule, &conducting, 0.1).expect("ok");
        for w in [
            &small.vector_mean,
            &small.magnitude_mean,
            &small.magnitude_rms,
            &small.joule_mean,
        ] {
            assert!(w.undersampled, "0.1 < element edge: {w:?}");
        }
        let big = conduction_window_peaks(&mesh, &j, &joule, &conducting, 2.0).expect("ok");
        assert!(!big.vector_mean.undersampled);
        assert!(!big.joule_mean.undersampled);
    }

    #[test]
    fn conduction_window_errors() {
        let mesh = cube_tet_mesh(1, 1.0);
        let j = vec![[1.0, 0.0, 0.0]; mesh.n_tets()];
        let joule = vec![1.0; mesh.n_tets()];
        let none = vec![false; mesh.n_tets()];
        assert!(matches!(
            conduction_window_peaks(&mesh, &j, &joule, &none, 0.2),
            Err(PeakFieldError::EmptyRegion)
        ));
        let conducting = vec![true; mesh.n_tets()];
        for edge in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                conduction_window_peaks(&mesh, &j, &joule, &conducting, edge),
                Err(PeakFieldError::BadWindowEdge(_))
            ));
        }
    }
}
