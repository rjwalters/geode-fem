//! Structured **polar sector meshes** with tagged boundary edges, for the
//! sector and sliding-band machinery of
//! [`crate::assembly::magnetostatic_sector`] (motor-paper milestone M2,
//! issue #1161).
//!
//! A [`PolarSectorSpec`] describes concentric bands `radii[b] ≤ r ≤
//! radii[b+1]`, each with its own number of radial layers and triangle tag,
//! over an angular span `[θ0, θ0 + span]`. The span is either a sector
//! (`span < 2π`, with two radial edges) or the full circle (`span = 2π`,
//! no radial edges). Every ring carries the same `n_ang` angular cells, so
//! a circle at any band radius has `n_ang` uniformly spaced cells: the
//! conforming coupling circle of a sliding band.
//!
//! The mesh can be **split** at one band radius (`split_at`). The nodes on
//! that circle are then duplicated: the rotor (inner) triangles use one
//! copy and the stator (outer) triangles the other, as a sliding-band
//! mesher would produce. Unsplit meshes share them. Both layouts are
//! accepted by [`crate::assembly::magnetostatic_sector::SlidingBand`].
//!
//! Boundary edges are returned with the tags [`EDGE_INNER`], [`EDGE_OUTER`],
//! [`EDGE_LOW`] (θ = θ0), [`EDGE_HIGH`] (θ = θ0 + span) and
//! [`EDGE_COUPLING`] (the circle at `split_at`, listed once per side when
//! split). These play the role of gmsh physical curves.
//!
//! An optional deterministic **jitter** moves nodes strictly inside a band
//! (never on a band radius or a radial edge) by up to `jitter` of the local
//! cell size. It gives a genuinely different triangulation of the same
//! geometry, which the rotation tests use as the "fresh mesh".
//!
//! The triangulation is invariant under rotation by one angular cell (each
//! quad is split along the same diagonal), so an antiperiodic sector and the
//! full machine built with the same cell size share their nodes and
//! elements exactly.

use std::f64::consts::TAU;

use crate::analytic::waveguide::TriMesh;

/// Edge tag of the inner circle `r = radii[0]` (absent if `radii[0] = 0`).
pub const EDGE_INNER: i32 = 1;
/// Edge tag of the outer circle `r = radii[last]`.
pub const EDGE_OUTER: i32 = 2;
/// Edge tag of the radial edge at `θ = θ0` (sectors only).
pub const EDGE_LOW: i32 = 3;
/// Edge tag of the radial edge at `θ = θ0 + span` (sectors only).
pub const EDGE_HIGH: i32 = 4;
/// Edge tag of the coupling circle `r = radii[split_at]` (split meshes only,
/// and also emitted for an unsplit mesh when `coupling_at` is set).
pub const EDGE_COUPLING: i32 = 5;

/// Parameters of a structured polar sector mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarSectorSpec {
    /// Band radii, strictly increasing. `radii[0]` may be `0` (a centre
    /// node with a triangle fan).
    pub radii: Vec<f64>,
    /// Radial layers per band (`radii.len() − 1` entries, each ≥ 1).
    pub layers: Vec<usize>,
    /// Triangle tag per band (`radii.len() − 1` entries).
    pub band_tags: Vec<i32>,
    /// Angular cells over the span (≥ 1; ≥ 3 for the full circle).
    pub n_ang: usize,
    /// Start angle `θ0` (rad).
    pub theta0: f64,
    /// Angular span (rad): `0 < span < 2π` for a sector, or exactly `2π`.
    pub span: f64,
    /// Index into `radii` of the circle to split into rotor and stator
    /// copies (`None` for a shared, unsplit circle).
    pub split_at: Option<usize>,
    /// Index into `radii` of the circle emitted as [`EDGE_COUPLING`] in an
    /// unsplit mesh (ignored when `split_at` is set).
    pub coupling_at: Option<usize>,
    /// Interior-node jitter as a fraction of the local cell size (0 = none).
    pub jitter: f64,
    /// Seed of the jitter pattern.
    pub jitter_seed: u64,
}

/// A polar sector mesh with per-triangle and per-boundary-edge tags.
#[derive(Debug, Clone)]
pub struct PolarSectorMesh {
    /// The triangle mesh (CCW triangles).
    pub mesh: TriMesh,
    /// Per-triangle band tag.
    pub tri_tags: Vec<i32>,
    /// Tagged boundary (and coupling) edges.
    pub edges: Vec<[u32; 2]>,
    /// Tag per entry of `edges` ([`EDGE_INNER`], …).
    pub edge_tags: Vec<i32>,
    /// `node_rc[n] = (ring, j)`: the structured ring and angular index of
    /// node `n` (the centre is `(0, 0)` when `radii[0] = 0`).
    pub node_rc: Vec<(usize, usize)>,
}

impl PolarSectorMesh {
    /// Nodes carrying at least one edge with a tag in `tags`, sorted and
    /// de-duplicated.
    pub fn nodes_with_edge_tags(&self, tags: &[i32]) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .edges
            .iter()
            .zip(&self.edge_tags)
            .filter(|(_, t)| tags.contains(t))
            .flat_map(|(e, _)| [e[0], e[1]])
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn splitmix(mut z: u64) -> f64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// Build a structured polar sector mesh.
///
/// # Panics
///
/// Panics on an inconsistent spec: fewer than two radii, non-increasing or
/// negative radii, a length mismatch of `layers` / `band_tags`, a zero layer
/// count, `n_ang` too small, a span outside `(0, 2π]`, a `split_at` /
/// `coupling_at` that is not an interior band radius, or a negative jitter
/// of at least 0.5.
#[allow(clippy::needless_range_loop)]
pub fn polar_sector_mesh(spec: &PolarSectorSpec) -> PolarSectorMesh {
    let nb = spec.radii.len().saturating_sub(1);
    assert!(nb >= 1, "polar_sector_mesh: need at least two radii");
    assert!(
        spec.radii[0] >= 0.0 && spec.radii.windows(2).all(|w| w[0] < w[1]),
        "polar_sector_mesh: radii must be >= 0 and strictly increasing: {:?}",
        spec.radii
    );
    assert_eq!(spec.layers.len(), nb, "polar_sector_mesh: layers length");
    assert_eq!(
        spec.band_tags.len(),
        nb,
        "polar_sector_mesh: band_tags length"
    );
    assert!(
        spec.layers.iter().all(|&l| l >= 1),
        "polar_sector_mesh: layers must be >= 1"
    );
    let full = (spec.span - TAU).abs() <= 1e-12;
    assert!(
        full || (spec.span > 0.0 && spec.span < TAU),
        "polar_sector_mesh: span must be in (0, 2pi] (got {})",
        spec.span
    );
    assert!(
        spec.n_ang >= if full { 3 } else { 1 },
        "polar_sector_mesh: n_ang too small"
    );
    for at in [spec.split_at, spec.coupling_at].into_iter().flatten() {
        assert!(
            at >= 1 && at < nb,
            "polar_sector_mesh: split_at / coupling_at must be an interior band radius index"
        );
    }
    assert!(
        (0.0..0.5).contains(&spec.jitter),
        "polar_sector_mesh: jitter must be in [0, 0.5)"
    );

    // Ring radii and the band of each ring.
    let mut ring_r = vec![spec.radii[0]];
    let mut ring_on_band_radius = vec![Some(0usize)];
    let mut ring_band = vec![0usize];
    for b in 0..nb {
        let (r0, r1) = (spec.radii[b], spec.radii[b + 1]);
        for l in 1..=spec.layers[b] {
            ring_r.push(r0 + (r1 - r0) * l as f64 / spec.layers[b] as f64);
            ring_band.push(b);
            ring_on_band_radius.push((l == spec.layers[b]).then_some(b + 1));
        }
    }
    // Make band radii exact.
    for (i, rb) in ring_on_band_radius.iter().enumerate() {
        if let Some(b) = rb {
            ring_r[i] = spec.radii[*b];
        }
    }
    let n_rings = ring_r.len();
    let has_centre = spec.radii[0] == 0.0;
    let n_per_ring = if full { spec.n_ang } else { spec.n_ang + 1 };
    let dth = spec.span / spec.n_ang as f64;
    let split_ring = spec.split_at.map(|b| {
        ring_on_band_radius
            .iter()
            .position(|x| *x == Some(b))
            .unwrap()
    });
    let coupling_ring = spec.split_at.or(spec.coupling_at).map(|b| {
        ring_on_band_radius
            .iter()
            .position(|x| *x == Some(b))
            .unwrap()
    });

    let mut nodes: Vec<[f64; 2]> = Vec::new();
    let mut node_rc = Vec::new();
    // id[ring][j] (stator copy on the split ring); rotor_id for the split ring.
    let mut id = vec![vec![u32::MAX; n_per_ring]; n_rings];
    let mut rotor_split = vec![u32::MAX; n_per_ring];
    for (i, &r) in ring_r.iter().enumerate() {
        if i == 0 && has_centre {
            nodes.push([0.0, 0.0]);
            node_rc.push((0, 0));
            for slot in id[0].iter_mut() {
                *slot = 0;
            }
            continue;
        }
        let interior_ring = ring_on_band_radius[i].is_none();
        for j in 0..n_per_ring {
            let mut rr = r;
            let mut th = spec.theta0 + dth * j as f64;
            let on_radial_edge = !full && (j == 0 || j == spec.n_ang);
            if spec.jitter > 0.0 && interior_ring && !on_radial_edge {
                let dr = (ring_r[i + 1] - ring_r[i]).min(ring_r[i] - ring_r[i - 1]);
                let key = spec.jitter_seed ^ ((i as u64) << 32) ^ j as u64;
                rr += spec.jitter * dr * splitmix(key);
                th += spec.jitter * dth * splitmix(key.wrapping_mul(31).wrapping_add(7));
            }
            id[i][j] = nodes.len() as u32;
            nodes.push([rr * th.cos(), rr * th.sin()]);
            node_rc.push((i, j));
        }
        if Some(i) == split_ring {
            for j in 0..n_per_ring {
                let th = spec.theta0 + dth * j as f64;
                rotor_split[j] = nodes.len() as u32;
                nodes.push([r * th.cos(), r * th.sin()]);
                node_rc.push((i, j));
            }
        }
    }
    let wrap = |j: usize| if full { j % spec.n_ang } else { j };
    // Node of ring i, angular index j, as seen from band `band` (the rotor
    // side of the split ring uses the rotor copy).
    let node_of = |i: usize, j: usize, inner_side: bool| -> u32 {
        let j = wrap(j);
        if Some(i) == split_ring && inner_side {
            rotor_split[j]
        } else {
            id[i][j]
        }
    };

    let mut tris = Vec::new();
    let mut tri_tags = Vec::new();
    for i in 0..n_rings - 1 {
        let band = ring_band[i + 1];
        let tag = spec.band_tags[band];
        // Triangles between ring i and i+1 belong to `band`; whether they
        // are inside the split circle decides which copy they use.
        let inner = spec.split_at.is_some_and(|b| band < b);
        for j in 0..spec.n_ang {
            let a = node_of(i + 1, j, inner);
            let b = node_of(i + 1, j + 1, inner);
            if i == 0 && has_centre {
                tris.push([0, a, b]);
                tri_tags.push(tag);
                continue;
            }
            let c = node_of(i, j, inner);
            let d = node_of(i, j + 1, inner);
            // Quad c-d-b-a, split along c–b; both triangles CCW.
            tris.push([c, b, d]);
            tris.push([c, a, b]);
            tri_tags.push(tag);
            tri_tags.push(tag);
        }
    }

    let mut edges = Vec::new();
    let mut edge_tags = Vec::new();
    let mut ring_edges = |i: usize, inner: bool, tag: i32, edges: &mut Vec<[u32; 2]>| {
        for j in 0..spec.n_ang {
            edges.push([node_of(i, j, inner), node_of(i, j + 1, inner)]);
            edge_tags.push(tag);
        }
    };
    if !has_centre {
        ring_edges(0, false, EDGE_INNER, &mut edges);
    }
    ring_edges(n_rings - 1, false, EDGE_OUTER, &mut edges);
    if let Some(cr) = coupling_ring {
        ring_edges(cr, false, EDGE_COUPLING, &mut edges);
        if split_ring.is_some() {
            ring_edges(cr, true, EDGE_COUPLING, &mut edges);
        }
    }
    if !full {
        for (j, tag) in [(0usize, EDGE_LOW), (spec.n_ang, EDGE_HIGH)] {
            for i in 0..n_rings - 1 {
                let band = ring_band[i + 1];
                let inner = spec.split_at.is_some_and(|b| band < b);
                edges.push([node_of(i, j, inner), node_of(i + 1, j, inner)]);
                edge_tags.push(tag);
            }
        }
    }
    PolarSectorMesh {
        mesh: TriMesh { nodes, tris },
        tri_tags,
        edges,
        edge_tags,
        node_rc,
    }
}
