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

use crate::mesh::{TaggedTetMesh, TetMesh};
use crate::postproc::sample::{Aabb, sample_cell_average};

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
}
