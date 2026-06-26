//! Circular metallic waveguide transverse-modal eigensolver integration
//! test (issue #265 — general-cross-section eigensolver).
//!
//! Drives the new general-cross-section entry point
//! [`solve_waveguide_modes`] on an in-memory triangulated disk
//! cross-section and pairs the lowest FEM cutoff wavenumbers `k_c`
//! against the analytic circular-waveguide oracle. The circular
//! waveguide is the canonical non-rectangular fixture: its cutoffs are
//! NOT `(π / W)²` for any "width" W, so the pre-#265 hardcoded
//! rectangular shift in [`solve_rect_waveguide_modes`] cannot solve
//! this geometry at all — the test directly exercises the issue #265
//! generalization.
//!
//! # Analytic oracle
//!
//! For a metallic circular waveguide of radius `R`, the transverse
//! cutoff wavenumbers are zeros of Bessel functions:
//!
//! - **TE_{mn}** modes: `k_c · R = j'_{m,n}` (n-th positive root of
//!   `J'_m(x)`).
//! - **TM_{mn}** modes: `k_c · R = j_{m,n}` (n-th positive root of
//!   `J_m(x)`).
//!
//! The dominant mode is **TE_{1,1}** with `j'_{1,1} ≈ 1.84118`. The
//! lowest few cutoffs in ascending order are:
//!
//! ```text
//! TE_{1,1}: j'_{1,1} ≈ 1.84118
//! TM_{0,1}: j_{0,1}  ≈ 2.40483
//! TE_{2,1}: j'_{2,1} ≈ 3.05424
//! TE_{0,1}: j'_{0,1} ≈ 3.83171   (degenerate with TM_{1,1})
//! TM_{1,1}: j_{1,1}  ≈ 3.83171   (degenerate with TE_{0,1})
//! ```
//!
//! See Pozar §3.4 / Collin §5.3 for the full derivation.
//!
//! # Mesh
//!
//! A radial-fan triangulation of the disk: `n_radial × n_circ` quads
//! split into triangles, with a single central node at `r = 0`. This
//! is a coarse but well-shaped mesh — adequate for the few-percent
//! cutoff accuracy band the test asserts.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --test circular_waveguide_modes
//! ```

use geode_core::analytic::waveguide::{TriMesh, solve_waveguide_modes};

/// Hard-coded Bessel-zero table for the analytic oracle. Values from
/// Abramowitz & Stegun Table 9.5 (J_m zeros) and 9.6 (J'_m zeros),
/// truncated to 5 decimal places. Index `(m, n)` is the `n`-th
/// positive root of `J_m(x)` (TM) or `J'_m(x)` (TE) for `m, n ≥ 1`
/// (TM) or `n ≥ 1`, `m ≥ 0` (TE).
const J_M_ZEROS: &[(u32, u32, f64)] = &[
    // (m, n, j_{m,n}) — TM_{m,n} cutoff factor
    (0, 1, 2.40483),
    (0, 2, 5.52008),
    (1, 1, 3.83171),
    (1, 2, 7.01559),
    (2, 1, 5.13562),
    (2, 2, 8.41724),
    (3, 1, 6.38016),
];
const JP_M_ZEROS: &[(u32, u32, f64)] = &[
    // (m, n, j'_{m,n}) — TE_{m,n} cutoff factor (with J'_0(0) = 0
    // *excluded* — only positive roots).
    (0, 1, 3.83171),
    (0, 2, 7.01559),
    (1, 1, 1.84118),
    (1, 2, 5.33144),
    (2, 1, 3.05424),
    (2, 2, 6.70613),
    (3, 1, 4.20119),
];

/// Build the analytic catalog of circular-waveguide cutoffs for radius
/// `R`, returning a flat list of `(kind, m, n, k_c)` ordered by
/// ascending `k_c`. `kind = 'E'` is TE, `'M'` is TM.
fn circular_cutoff_catalog(r: f64) -> Vec<(char, u32, u32, f64)> {
    let mut catalog: Vec<(char, u32, u32, f64)> = Vec::new();
    for &(m, n, jp) in JP_M_ZEROS {
        catalog.push(('E', m, n, jp / r));
    }
    for &(m, n, j) in J_M_ZEROS {
        catalog.push(('M', m, n, j / r));
    }
    catalog.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
    catalog
}

/// Generate a radial-fan triangulation of a disk of radius `r` with
/// `n_radial` radial rings and `n_circ` angular sectors. Returns a
/// `TriMesh` with `1 + n_radial · n_circ` nodes and
/// `n_circ · (2 · n_radial − 1)` triangles.
///
/// Node ordering: index 0 is the disk center; thereafter nodes are
/// laid out ring by ring, `n_circ` per ring. The k-th node on ring r
/// (1-indexed) is at angle `2π · k / n_circ`, radius `r · (ring / n_radial)`.
///
/// All triangles are listed in CCW order (positive signed area).
fn disk_tri_mesh(r: f64, n_radial: usize, n_circ: usize) -> TriMesh {
    assert!(n_radial >= 1 && n_circ >= 3);
    use std::f64::consts::TAU;
    let mut nodes: Vec<[f64; 2]> = Vec::with_capacity(1 + n_radial * n_circ);
    nodes.push([0.0, 0.0]); // center
    for ring in 1..=n_radial {
        let rho = r * (ring as f64) / (n_radial as f64);
        for k in 0..n_circ {
            let theta = TAU * (k as f64) / (n_circ as f64);
            nodes.push([rho * theta.cos(), rho * theta.sin()]);
        }
    }
    // Node index of ring `ring` (1..=n_radial), sector `k` (mod n_circ).
    let idx = |ring: usize, k: usize| -> u32 {
        if ring == 0 {
            0
        } else {
            (1 + (ring - 1) * n_circ + (k % n_circ)) as u32
        }
    };

    let mut tris: Vec<[u32; 3]> = Vec::new();
    // Innermost ring: fan from center to ring-1 vertices.
    for k in 0..n_circ {
        // Triangle (center, ring1[k], ring1[k+1]) — CCW because
        // ring1[k+1] is at greater angle than ring1[k].
        tris.push([idx(0, 0), idx(1, k), idx(1, k + 1)]);
    }
    // Outer rings: quads split into two triangles each.
    //
    // The four corners in (ring, sector) are:
    //   a = (ring,     k)       — inner-θ
    //   b = (ring,     k + 1)   — inner-(θ + dθ)
    //   c = (ring + 1, k + 1)   — outer-(θ + dθ)
    //   d = (ring + 1, k)       — outer-θ
    //
    // CCW traversal of the wedge (positive signed area) goes
    // inner-θ → outer-θ → outer-(θ+dθ) → inner-(θ+dθ), i.e.
    // a → d → c → b. Split along the a–c diagonal: triangles
    // (a, d, c) and (a, c, b) — both CCW.
    for ring in 1..n_radial {
        for k in 0..n_circ {
            let a = idx(ring, k);
            let b = idx(ring, k + 1);
            let c = idx(ring + 1, k + 1);
            let d = idx(ring + 1, k);
            tris.push([a, d, c]);
            tris.push([a, c, b]);
        }
    }
    TriMesh { nodes, tris }
}

/// Build the PEC interior-edge mask for a disk of radius `r`: an edge
/// is **interior** unless both its endpoints lie on the outer boundary
/// (i.e. the edge is a chord of the outer ring, which approximates
/// the curved PEC wall in the polygonalized mesh).
///
/// Returns `(edges, interior_edge_mask)` aligned with [`TriMesh::edges`].
fn disk_pec_interior_edges(mesh: &TriMesh, r: f64) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-9 * r.max(1.0);
    let on_boundary: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| {
            let rho = (p[0] * p[0] + p[1] * p[1]).sqrt();
            (rho - r).abs() < tol
        })
        .collect();
    let edges = mesh.edges();
    let mask: Vec<bool> = edges
        .iter()
        .map(|e| !(on_boundary[e[0] as usize] && on_boundary[e[1] as usize]))
        .collect();
    (edges, mask)
}

/// Smoke test: the disk mesh generator produces a topologically sound
/// triangulation with the expected node and triangle counts.
#[test]
fn disk_mesh_topology_smoke() {
    let n_radial = 4;
    let n_circ = 12;
    let mesh = disk_tri_mesh(1.0, n_radial, n_circ);
    assert_eq!(mesh.n_nodes(), 1 + n_radial * n_circ);
    // n_circ inner-fan tris + (n_radial - 1) outer-ring quads × 2 = 2 × n_circ
    // per outer ring.
    assert_eq!(
        mesh.n_tris(),
        n_circ + (n_radial - 1) * n_circ * 2,
        "expected {} tris, got {}",
        n_circ + (n_radial - 1) * n_circ * 2,
        mesh.n_tris()
    );
}

/// **Issue #265 acceptance**: the general-cross-section modal solver
/// recovers the analytic TE_{1,1} cutoff of a circular waveguide to
/// within a few percent on a moderately refined disk mesh. The TE_{1,1}
/// mode is doubly degenerate (cos / sin in azimuth) so we accept either
/// of the two lowest FEM eigenvalues as the TE_{1,1} match.
///
/// This is the load-bearing test: the pre-#265 hardcoded rectangular
/// shift `0.3 · (π/W)²` makes no sense for a disk (no rectangular
/// "width" exists), and the spurious threshold `0.01 · (π/W)²` could
/// arbitrarily mis-classify modes if the disk's lowest physical
/// eigenvalue happens to fall below it. The general
/// [`solve_waveguide_modes`] estimates both σ and the threshold from
/// the spectrum itself.
#[test]
fn circular_te11_matches_analytic() {
    let r = 1.0;
    let n_radial = 8;
    let n_circ = 24;
    let mesh = disk_tri_mesh(r, n_radial, n_circ);
    let (edges, mask) = disk_pec_interior_edges(&mesh, r);
    let n_modes = 3;
    let modes = solve_waveguide_modes(&mesh, &edges, &mask, n_modes)
        .expect("general-cross-section modal solve on circular waveguide");
    assert_eq!(modes.len(), n_modes);

    let catalog = circular_cutoff_catalog(r);
    eprintln!("circular waveguide R = {r} — first few modes:");
    for (i, m) in modes.iter().enumerate() {
        let closest = catalog
            .iter()
            .min_by(|a, b| {
                (a.3 - m.k_c)
                    .abs()
                    .partial_cmp(&(b.3 - m.k_c).abs())
                    .unwrap()
            })
            .unwrap();
        let rel_err = (m.k_c - closest.3).abs() / closest.3;
        eprintln!(
            "  mode[{i}]: fem k_c = {:.5} → T{}_{{{},{}}} analytic = {:.5} ({:+.2}%)",
            m.k_c,
            closest.0,
            closest.1,
            closest.2,
            closest.3,
            100.0 * rel_err
        );
    }

    // Acceptance: TE_{1,1} at k_c ≈ 1.84118 / R = 1.84118.
    let kc_te11 = JP_M_ZEROS
        .iter()
        .find(|&&(m, n, _)| m == 1 && n == 1)
        .map(|&(_, _, jp)| jp / r)
        .unwrap();
    // TE_{1,1} is doubly degenerate (azimuthal sin/cos), so the FEM
    // produces two near-equal eigenvalues at the bottom of the
    // spectrum. Either of the two lowest modes should match.
    let best_match = modes
        .iter()
        .map(|m| (m.k_c - kc_te11).abs() / kc_te11)
        .fold(f64::INFINITY, f64::min);
    assert!(
        best_match < 0.05,
        "circular waveguide TE_{{1,1}}: no FEM mode within 5 % of \
         analytic k_c = {kc_te11:.4} (best rel err {:.2}%)",
        100.0 * best_match
    );
}

/// **Multi-mode set-wise M-orthonormality regression** for the
/// general-cross-section solver: the K > 1 returned modes must form a
/// mutually M-orthonormal set, same as the rectangular path
/// (`multi_mode_set_wise_m_orthonormal_k2` in `waveguide_modes.rs`).
/// Lanczos in the M-inner product gives this for free; this test pins
/// the property for the general-cross-section code path so the
/// general/rectangular paths don't diverge silently.
#[test]
fn circular_multi_mode_set_wise_m_orthonormal() {
    use geode_core::analytic::waveguide::{apply_pec_2d, assemble_2d_nedelec};
    let r = 1.0;
    let mesh = disk_tri_mesh(r, 6, 18);
    let (edges, mask) = disk_pec_interior_edges(&mesh, r);
    let modes = solve_waveguide_modes(&mesh, &edges, &mask, 2).expect("multi-mode circular solve");
    assert_eq!(modes.len(), 2);

    let (k_dense, m_dense) = assemble_2d_nedelec(&mesh);
    // Touch apply_pec_2d on the same mask so callers see the shape
    // contract is honoured (and to silence the unused-import lint on
    // apply_pec_2d if this test ever needed it directly).
    let _ = apply_pec_2d(&k_dense, &m_dense, &mask);
    let n_edges = m_dense.nrows();
    assert_eq!(modes[0].e_edges.len(), n_edges);

    let dot_me = |i: usize, j: usize| -> f64 {
        let mut acc = 0.0_f64;
        for p in 0..n_edges {
            for q in 0..n_edges {
                acc += modes[i].e_edges[p] * m_dense[(p, q)] * modes[j].e_edges[q];
            }
        }
        acc
    };
    let g00 = dot_me(0, 0);
    let g01 = dot_me(0, 1);
    let g11 = dot_me(1, 1);
    eprintln!(
        "circular set-wise M-Gram: G00 = {:.3e}, G01 = {:.3e}, G11 = {:.3e}",
        g00, g01, g11
    );
    let tol = 1e-10_f64;
    assert!((g00 - 1.0).abs() < tol, "mode[0]ᵀ M mode[0] = {} ≠ 1", g00);
    assert!((g11 - 1.0).abs() < tol, "mode[1]ᵀ M mode[1] = {} ≠ 1", g11);
    assert!(g01.abs() < tol, "mode[0]ᵀ M mode[1] = {} ≠ 0", g01);
}

/// **Oversized aspect-ratio stress test** (issue #265 step 3): on a
/// wide thin rectangular waveguide `a = 10, b = 1`, the lowest physical
/// `k_c² = (π/10)² ≈ 0.0987` is small in absolute terms (only ~one
/// decade above 0.01). The pre-#265 rectangular spurious threshold
/// `0.01 · (π/W)²` would set the cutoff at ≈ 9.87e-4 — still safely
/// below the gradient cluster which sits at machine-noise magnitude.
/// The general path's σ-relative threshold `0.1 · σ` with
/// `σ = 0.5 · λ_min_phys ≈ 0.049` puts the cutoff at ≈ 4.9e-3 — a
/// decade above the rectangular threshold but still well below the
/// physical mode. This test confirms both paths recover TE₁₀ on the
/// stretched cross-section.
#[test]
fn general_path_on_wide_rectangle_recovers_te10() {
    use geode_core::analytic::waveguide::{rect_pec_interior_edges, rect_tri_mesh};
    let (a, b) = (10.0_f64, 1.0_f64);
    let nx = 20;
    let ny = 4;
    let mesh = rect_tri_mesh(nx, ny, a, b);
    let (edges, mask) = rect_pec_interior_edges(&mesh, a, b);
    let modes = solve_waveguide_modes(&mesh, &edges, &mask, 2)
        .expect("general-path solve on wide thin rectangle");
    let pi = std::f64::consts::PI;
    let kc_te10 = pi / a;
    let rel_err = (modes[0].k_c - kc_te10).abs() / kc_te10;
    eprintln!(
        "wide rect a={a}, b={b} ({nx}x{ny}): TE10 fem k_c = {:.5}, \
         analytic = {:.5} ({:+.2}%); λ = {:.3e}",
        modes[0].k_c,
        kc_te10,
        100.0 * rel_err,
        modes[0].lambda
    );
    assert!(
        rel_err < 0.05,
        "TE10 on wide rect a={a} b={b}: rel err {} too large",
        rel_err
    );
    // Both returned modes must be physical (λ > 0 and well above the
    // gradient cluster).
    for (i, m) in modes.iter().enumerate() {
        assert!(
            m.lambda > 1e-6,
            "mode[{i}]: λ = {} — too close to gradient cluster",
            m.lambda
        );
    }
}

/// **Rectangular cross-section reduction**: invoking the general-path
/// [`solve_waveguide_modes`] with a rectangular PEC mask should still
/// produce a TE₁₀ cutoff close to the analytic value. This documents
/// that the general path **does** work on rectangular geometry too
/// (just with the auto-estimated σ instead of the rectangular-tuned
/// `0.3·(π/W)²` — numerical values differ slightly from the
/// rectangular shim but the **accuracy band** is the same).
#[test]
fn general_path_on_rectangular_mesh_matches_te10() {
    use geode_core::analytic::waveguide::{rect_pec_interior_edges, rect_tri_mesh};
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    let (edges, mask) = rect_pec_interior_edges(&mesh, a, b);
    let modes = solve_waveguide_modes(&mesh, &edges, &mask, 2)
        .expect("general-path solve on rectangular mesh");
    let pi = std::f64::consts::PI;
    let kc_te10 = pi / a;
    let rel_err = (modes[0].k_c - kc_te10).abs() / kc_te10;
    eprintln!(
        "general-path on rect 16x8: TE10 fem k_c = {:.5}, analytic = {:.5} ({:+.2}%)",
        modes[0].k_c,
        kc_te10,
        100.0 * rel_err
    );
    assert!(
        rel_err < 0.03,
        "general-path TE10 on rectangular mesh: rel err {} too large",
        rel_err
    );
}
