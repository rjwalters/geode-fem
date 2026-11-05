use super::*;

/// Issue #831: `principal_sqrt_c64` recovers the propagation loss
/// `Im n_eff` of a weakly leaky mode to a few ulps, at `Im β² / Re β² =
/// 1e-6` and `1e-10`, in µm (`β² ≈ 21.8 µm⁻²`) and metre (`≈ 2.18e13
/// m⁻²`) units. References: `mpmath` at 50 digits on the exact binary64
/// `β²` (rounded to binary64). The old cancelling form was off by `1.3e-5` / `1.1e-4` at
/// `1e-6` and returned `Im n_eff = 0` at `1e-10`.
#[test]
fn principal_sqrt_c64_keeps_low_loss_im_n_eff() {
    let k0 = 4.05_f64;
    // (Re β², Im/Re, mpmath Im n_eff, mpmath Re n_eff)
    let cases = [
        (21.8, 1e-6, 5.764255570334466e-07, 1.1528511140671813),
        (21.8, 1e-10, 5.764255570335187e-11, 1.1528511140670372),
        (2.18e13, 1e-6, 0.5764255570334466, 1152851.1140671815),
        (2.18e13, 1e-10, 5.7642555703351866e-05, 1152851.1140670374),
    ];
    for (re, ratio, im_ref, re_ref) in cases {
        let beta_sq = c64::new(re, re * ratio);
        let n_eff = principal_sqrt_c64(beta_sq) / c64::new(k0, 0.0);
        let rel_im = (n_eff.im - im_ref).abs() / im_ref;
        let rel_re = (n_eff.re - re_ref).abs() / re_ref;
        assert!(
            rel_im <= 4.0 * f64::EPSILON && rel_re <= 4.0 * f64::EPSILON,
            "β² = {beta_sq}: n_eff = {n_eff}, rel err Im {rel_im:.2e}, Re {rel_re:.2e}"
        );
        // The leaky sign is kept, and the lossless branch stays exact.
        let conj = principal_sqrt_c64(c64::new(re, -re * ratio));
        assert!(conj.im < 0.0 && conj.re == principal_sqrt_c64(beta_sq).re);
    }
    assert_eq!(
        principal_sqrt_c64(c64::new(21.8, 0.0)).im.to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(principal_sqrt_c64(c64::new(-4.0, 0.0)), c64::new(0.0, 2.0));
    assert_eq!(principal_sqrt_c64(c64::new(0.0, 0.0)), c64::new(0.0, 0.0));
}

/// The 6-point degree-4 rule integrates every barycentric monomial of
/// total degree ≤ 4 exactly against the closed form on the reference
/// triangle `∫_T λ₀^a λ₁^b λ₂^c dA = a! b! c! / (a+b+c+2)! · 2|T|`
/// (with `|T| = 1/2` for the unit reference triangle, so the factor is
/// `a! b! c! / (a+b+c+2)!`).
#[test]
fn tri_quad_deg4_integrates_polynomials_exactly() {
    // weights sum to 1 (normalised to element area).
    let wsum: f64 = TRI_QUAD_DEG4.iter().map(|r| r[3]).sum();
    assert!((wsum - 1.0).abs() < 1e-12, "weights must sum to 1: {wsum}");

    fn fact(n: u32) -> f64 {
        (1..=n).map(|k| k as f64).product::<f64>().max(1.0)
    }
    // closed form of ∫_T λ₀^a λ₁^b λ₂^c dA over the *reference* triangle
    // (area 1/2): a!b!c!/(a+b+c+2)! .
    let exact =
        |a: u32, b: u32, c: u32| -> f64 { fact(a) * fact(b) * fact(c) / fact(a + b + c + 2) };

    let ref_area = 0.5_f64;
    for a in 0..=4u32 {
        for b in 0..=(4 - a) {
            for c in 0..=(4 - a - b) {
                if a + b + c > 4 {
                    continue;
                }
                let num: f64 = TRI_QUAD_DEG4
                    .iter()
                    .map(|r| r[3] * r[0].powi(a as i32) * r[1].powi(b as i32) * r[2].powi(c as i32))
                    .sum::<f64>()
                    * ref_area;
                let want = exact(a, b, c);
                assert!(
                    (num - want).abs() < 1e-13,
                    "deg-4 quad wrong for λ0^{a} λ1^{b} λ2^{c}: got {num}, want {want}"
                );
            }
        }
    }
}

/// A degree-5 monomial is NOT integrated exactly (guards against an
/// accidentally-too-strong rule masking a basis-degree mistake).
#[test]
fn tri_quad_deg4_misses_degree5() {
    let ref_area = 0.5_f64;
    // ∫ λ0^5 dA = 5!*0!*0!/7! = 1/42 over the reference triangle.
    let num: f64 = TRI_QUAD_DEG4
        .iter()
        .map(|r| r[3] * r[0].powi(5))
        .sum::<f64>()
        * ref_area;
    let want = 1.0 / 42.0;
    assert!(
        (num - want).abs() > 1e-6,
        "degree-4 rule should not be exact at degree 5"
    );
}

/// **p=1 subset (load-bearing):** the 3×3 sub-block of the p=2 `K`/`M`
/// over the three Whitney (first-edge) DOFs `{0, 2, 4}` must equal the
/// closed-form first-order `tri_nedelec_local` kernel.
#[test]
fn p2_whitney_subblock_matches_p1() {
    // A deliberately non-degenerate, non-reference triangle.
    let coords = [[0.3, -0.2], [1.7, 0.1], [0.6, 1.4]];
    let (k1, m1, a1) = tri_nedelec_local(&coords);
    let (k2, m2, a2) = tri_nedelec2_local(&coords);
    assert!((a1 - a2).abs() < 1e-14, "areas must match: {a1} vs {a2}");

    let whitney = [0usize, 2, 4];
    for (i, &gi) in whitney.iter().enumerate() {
        for (j, &gj) in whitney.iter().enumerate() {
            assert!(
                (k2[gi][gj] - k1[i][j]).abs() < 1e-10,
                "K subblock mismatch at ({i},{j}): {} vs {}",
                k2[gi][gj],
                k1[i][j]
            );
            assert!(
                (m2[gi][gj] - m1[i][j]).abs() < 1e-10,
                "M subblock mismatch at ({i},{j}): {} vs {}",
                m2[gi][gj],
                m1[i][j]
            );
        }
    }
}

/// `K` and `M` are symmetric to tight tolerance.
#[test]
fn p2_local_matrices_symmetric() {
    let coords = [[0.0, 0.0], [2.1, 0.3], [0.4, 1.9]];
    let (k, m, _) = tri_nedelec2_local(&coords);
    for i in 0..8 {
        for j in 0..8 {
            assert!(
                (k[i][j] - k[j][i]).abs() < 1e-12,
                "K not symmetric at ({i},{j})"
            );
            assert!(
                (m[i][j] - m[j][i]).abs() < 1e-12,
                "M not symmetric at ({i},{j})"
            );
        }
    }
}

/// Single-element sanity checks.
///
/// 1. The gradient edge functions `Q = ∇(λ_a λ_b)` carry zero curl, so
///    their `K` diagonal entries (and any coefficient vector supported
///    only on the curl-free DOFs `{1, 3, 5}`) yield zero curl energy.
/// 2. A constant-curl field is integrated correctly: the curl-energy
///    of a unit Whitney DOF equals `(∇×W)² · |T|`.
#[test]
fn p2_local_sanity_checks() {
    let coords = [[0.1, 0.0], [1.2, -0.1], [0.5, 1.3]];
    let (k, _m, area) = tri_nedelec2_local(&coords);
    let area_abs = area.abs();

    // (1) curl-free gradient DOFs → zero curl energy.
    for &q in &[1usize, 3, 5] {
        assert!(
            k[q][q].abs() < 1e-12,
            "gradient DOF {q} should have zero curl energy, got {}",
            k[q][q]
        );
    }
    // A mixed gradient-only coefficient vector also gives zero energy.
    let mut e = 0.0;
    let coeff = [0.0, 1.3, 0.0, -0.7, 0.0, 2.1, 0.0, 0.0];
    for i in 0..8 {
        for j in 0..8 {
            e += coeff[i] * k[i][j] * coeff[j];
        }
    }
    assert!(
        e.abs() < 1e-11,
        "gradient-only curl energy must vanish: {e}"
    );

    // (2) constant-curl check: ∇×W₀ = 2 (g0 × g1)_z is constant, so
    // ∫ (∇×W₀)² dA = (∇×W₀)² |T| = K[0][0].
    let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
        - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
    let g0 = [
        (coords[1][1] - coords[2][1]) / det,
        (coords[2][0] - coords[1][0]) / det,
    ];
    let g1 = [
        (coords[2][1] - coords[0][1]) / det,
        (coords[0][0] - coords[2][0]) / det,
    ];
    let curl_w0 = 2.0 * (g0[0] * g1[1] - g0[1] * g1[0]);
    let want = curl_w0 * curl_w0 * area_abs;
    assert!(
        (k[0][0] - want).abs() < 1e-12,
        "constant-curl integration wrong: K[0][0]={} want {want}",
        k[0][0]
    );
}

#[test]
fn rect_tri_mesh_smoke() {
    let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
    assert_eq!(mesh.n_nodes(), 9);
    assert_eq!(mesh.n_tris(), 8);
    // Edge count = (nx+1)*ny + nx*(ny+1) + nx*ny  (horizontal +
    // vertical + diagonals) = 3*2 + 2*3 + 2*2 = 16.
    assert_eq!(mesh.edges().len(), 16);
}

/// Triangle signed area helper for the disk-mesh quality checks.
fn signed_area(mesh: &TriMesh, t: &[u32; 3]) -> f64 {
    let p0 = mesh.nodes[t[0] as usize];
    let p1 = mesh.nodes[t[1] as usize];
    let p2 = mesh.nodes[t[2] as usize];
    let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
    0.5 * (e1[0] * e2[1] - e1[1] * e2[0])
}

/// Triangle aspect ratio = longest edge / shortest altitude
/// (= longest_edge² · √3 / (4·area) for the inradius-free form we use
/// here: ratio of the longest edge to twice the inradius). A value
/// near 1 is equilateral; large values flag slivers.
fn aspect_ratio(mesh: &TriMesh, t: &[u32; 3]) -> f64 {
    let p = [
        mesh.nodes[t[0] as usize],
        mesh.nodes[t[1] as usize],
        mesh.nodes[t[2] as usize],
    ];
    let len = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let l01 = len(p[0], p[1]);
    let l12 = len(p[1], p[2]);
    let l20 = len(p[2], p[0]);
    let longest = l01.max(l12).max(l20);
    let area = signed_area(mesh, t).abs();
    // inradius r = area / s, s = semiperimeter. ratio = longest / (2r).
    let s = 0.5 * (l01 + l12 + l20);
    let inradius = area / s;
    longest / (2.0 * inradius)
}

#[test]
fn disk_tri_mesh_counts_scale_with_resolution() {
    // n_rings = 2*n_radial annular layers; central fan = n_angular
    // triangles; each outer ring = 2*n_angular triangles.
    // tris = n_angular + (n_rings-1)*2*n_angular = n_angular*(4*n_radial-1).
    // nodes = 1 + n_rings*n_angular = 1 + 2*n_radial*n_angular.
    for &(nr, na) in &[(2usize, 8usize), (3, 12), (4, 24)] {
        let (mesh, tags) = disk_tri_mesh(1.0, 3.0, nr, na);
        assert_eq!(mesh.n_nodes(), 1 + 2 * nr * na);
        assert_eq!(mesh.n_tris(), na * (4 * nr - 1));
        assert_eq!(tags.len(), mesh.n_tris());
    }
    // Counts grow with each knob.
    let (m_small, _) = disk_tri_mesh(1.0, 3.0, 2, 8);
    let (m_more_r, _) = disk_tri_mesh(1.0, 3.0, 4, 8);
    let (m_more_a, _) = disk_tri_mesh(1.0, 3.0, 2, 16);
    assert!(m_more_r.n_tris() > m_small.n_tris());
    assert!(m_more_a.n_tris() > m_small.n_tris());
}

#[test]
fn disk_tri_mesh_triangles_ccw_and_non_degenerate() {
    // Balanced knobs (n_angular ≈ 2π·n_radial): the documented regime
    // that keeps the near-hub radial elongation under control.
    let (mesh, _) = disk_tri_mesh(1.0, 3.0, 4, 25);
    let mut min_area = f64::INFINITY;
    let mut max_aspect = 0.0_f64;
    for t in &mesh.tris {
        let a = signed_area(&mesh, t);
        assert!(
            a > 0.0,
            "triangle {t:?} not CCW / has non-positive area {a}"
        );
        min_area = min_area.min(a);
        max_aspect = max_aspect.max(aspect_ratio(&mesh, t));
    }
    assert!(min_area > 1e-12, "degenerate (near-zero-area) triangle");
    // Bounded aspect ratio (longest edge / 2·inradius). The worst
    // cells are the radially-elongated innermost ring; for balanced
    // knobs (≤ ~8 radial rings) this stays well under the documented
    // ~7 bound. The solver is sensitive to sliver anisotropy
    // (#305/#309), so this is a hard guard, not a soft sanity check.
    assert!(
        max_aspect < 7.0,
        "aspect ratio {max_aspect} exceeds sliver bound"
    );
}

#[test]
fn disk_tri_mesh_mesh_is_connected() {
    // Every node must be referenced by at least one triangle (no
    // orphan nodes), and the triangle graph (sharing nodes) must be a
    // single connected component.
    let (mesh, _) = disk_tri_mesh(1.0, 2.0, 3, 12);
    let mut used = vec![false; mesh.n_nodes()];
    for t in &mesh.tris {
        for &v in t {
            used[v as usize] = true;
        }
    }
    assert!(used.iter().all(|&u| u), "orphan node not used by any tri");

    // Union-find over nodes connected through shared triangles.
    let mut parent: Vec<usize> = (0..mesh.n_nodes()).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = x;
        while parent[c] != c {
            let n = parent[c];
            parent[c] = r;
            c = n;
        }
        r
    }
    for t in &mesh.tris {
        let a = find(&mut parent, t[0] as usize);
        let b = find(&mut parent, t[1] as usize);
        let c = find(&mut parent, t[2] as usize);
        parent[b] = a;
        parent[c] = a;
    }
    let root = find(&mut parent, 0);
    for v in 0..mesh.n_nodes() {
        assert_eq!(find(&mut parent, v), root, "mesh is disconnected");
    }
}

#[test]
fn disk_tri_mesh_region_tags_conform_to_core_circle() {
    let core_r = 1.0;
    let outer_r = 3.0;
    let (mesh, tags) = disk_tri_mesh(core_r, outer_r, 6, 48);
    // Tags are exactly {0, 1}.
    assert!(tags.iter().all(|&t| t == 0 || t == 1));
    // No triangle straddles the interface: for a core-tagged tri all
    // vertices have radius ≤ core_r (+tol); for cladding all vertices
    // have radius ≥ core_r (−tol). (Conforming ring boundary.)
    let tol = 1e-9 * outer_r;
    for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
        for &v in t {
            let p = mesh.nodes[v as usize];
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            if tag == 1 {
                assert!(r <= core_r + tol, "core tri vertex outside core: r={r}");
            } else {
                assert!(r >= core_r - tol, "cladding tri vertex inside core: r={r}");
            }
        }
    }
    // Area-fraction check: Σ core-triangle areas / total area ≈
    // π·core_r² / (π·outer_r²) = (core_r/outer_r)².
    let mut core_area = 0.0;
    let mut total_area = 0.0;
    for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
        let a = signed_area(&mesh, t);
        total_area += a;
        if tag == 1 {
            core_area += a;
        }
    }
    let expected = (core_r / outer_r).powi(2);
    let frac = core_area / total_area;
    // The core polygon and the outer polygon are both inscribed at the
    // SAME angular sampling, so the ratio of their areas is
    // (core_r/outer_r)² *exactly* — independent of n_angular — once the
    // core ring conforms. The only error is f64 round-off. A 1e-3 band
    // is generous for the polygon-area accumulation.
    assert!(
        (frac - expected).abs() < 1e-3,
        "core area fraction {frac} vs expected {expected}"
    );
}

#[test]
fn disk_tri_mesh_region_tags_feed_epsilon_helper() {
    // The tags must be consumable by the Phase-1A ε helper.
    let (_mesh, tags) = disk_tri_mesh(1.0, 2.0, 3, 16);
    let eps = epsilon_r_from_region_tags(&tags, |t| if t == 1 { 2.1 } else { 1.0 });
    assert_eq!(eps.len(), tags.len());
    assert!(eps.iter().all(|&e| e == 2.1 || e == 1.0));
    assert!(eps.contains(&2.1), "no core ε present");
    assert!(eps.contains(&1.0), "no cladding ε present");
}

#[test]
fn disk_boundary_set_is_identifiable() {
    let outer_r = 2.0;
    let n_angular = 16;
    let (mesh, _) = disk_tri_mesh(1.0, outer_r, 3, n_angular);
    let on_boundary = disk_boundary_nodes(&mesh, outer_r);
    // Exactly the outermost ring (n_angular nodes) is on the far wall.
    let n_boundary = on_boundary.iter().filter(|&&b| b).count();
    assert_eq!(n_boundary, n_angular);
    // The center node is interior.
    assert!(!on_boundary[0]);
    // PEC interior-edge mask: every gated (PEC) edge connects two
    // boundary nodes; at least one edge is interior and at least one
    // is PEC.
    let (edges, mask) = disk_pec_interior_edges(&mesh, outer_r);
    assert_eq!(edges.len(), mask.len());
    let n_interior = mask.iter().filter(|&&b| b).count();
    let n_pec = mask.len() - n_interior;
    assert!(n_interior > 0 && n_pec > 0);
    // The PEC edges are exactly the n_angular boundary-circle arcs.
    assert_eq!(n_pec, n_angular);
    // Interior-node mask is the complement of the boundary set.
    let interior_nodes = disk_pec_interior_nodes(&mesh, outer_r);
    for (i, (&on, &inside)) in on_boundary.iter().zip(interior_nodes.iter()).enumerate() {
        assert_eq!(on, !inside, "node {i} boundary/interior mismatch");
    }
}

#[test]
fn tri_local_signed_area_positive_for_ccw() {
    let coords = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let (_, _, area) = tri_nedelec_local(&coords);
    assert!((area - 0.5).abs() < 1e-15);
}

#[test]
fn tri_local_matrices_symmetric() {
    // Off-axis affine triangle.
    let coords = [[0.3, -0.1], [1.2, 0.4], [0.7, 1.1]];
    let (k, m, _) = tri_nedelec_local(&coords);
    for i in 0..3 {
        for j in (i + 1)..3 {
            assert!(
                (k[i][j] - k[j][i]).abs() < 1e-12,
                "K not symmetric: ({i},{j})"
            );
            assert!(
                (m[i][j] - m[j][i]).abs() < 1e-12,
                "M not symmetric: ({i},{j})"
            );
        }
    }
}

#[test]
fn pec_mask_excludes_only_boundary_edges() {
    let mesh = rect_tri_mesh(3, 3, 1.5, 0.7);
    let (edges, mask) = rect_pec_interior_edges(&mesh, 1.5, 0.7);
    assert_eq!(edges.len(), mask.len());
    let n_interior = mask.iter().filter(|&&b| b).count();
    let n_pec = mask.len() - n_interior;
    assert!(n_interior > 0);
    assert!(n_pec > 0);
    // Sanity: the rectangle boundary has 2*(3+3) = 12 edges in this
    // structured mesh (3 horizontal × 2 walls + 3 vertical × 2 walls).
    assert_eq!(n_pec, 12);
}

/// **Outgoing-wave β sign convention** (issue #254): under the
/// `+jωt` time convention, an outgoing wave is `exp(−jβz)` so a
/// below-cutoff (evanescent) β must have `Im(β) < 0` for `exp(−jβz)`
/// to **decay** for `z > 0`. The default complex `sqrt` branch
/// would give `Im(β) > 0` (a non-physical growing solution); this
/// test pins the corrected branch.
#[test]
fn waveguide_mode_profile_beta_complex_outgoing_branch() {
    let p = WaveguideModeProfile {
        k_c: 1.0,
        lambda: 1.0,
        e_edges: Vec::new(),
    };
    let c = 1.0;
    // Below cutoff (ω/c < k_c): β = −j √(k_c² − k²), Im < 0.
    let b = p.beta_complex(0.5, c);
    let expected_im = -(1.0_f64 - 0.25).sqrt();
    assert!(b.re == 0.0, "evanescent β must be pure imaginary");
    assert!(
        (b.im - expected_im).abs() < 1e-15,
        "evanescent β: Im = {} expected {}",
        b.im,
        expected_im
    );
    assert!(
        b.im < 0.0,
        "evanescent β must have Im < 0 (outgoing branch)"
    );
    // Verify exp(−jβz) decays for z > 0: e^{−jβz} where β = jb.im,
    // so −jβz = −j·(j·b.im)·z = b.im·z, exp() decays since b.im<0.
    let z = 2.0_f64;
    let exp_minus_jbz_magnitude = (b.im * z).exp();
    assert!(
        exp_minus_jbz_magnitude < 1.0,
        "exp(−jβz) must decay (magnitude {} < 1) for z = {} > 0",
        exp_minus_jbz_magnitude,
        z
    );

    // Above cutoff (ω/c > k_c): β = +√(k² − k_c²), real positive.
    let b = p.beta_complex(2.0, c);
    let expected_re = (4.0_f64 - 1.0).sqrt();
    assert!(b.im == 0.0, "propagating β must be pure real");
    assert!(
        (b.re - expected_re).abs() < 1e-15,
        "propagating β: Re = {} expected {}",
        b.re,
        expected_re
    );
    assert!(b.re > 0.0, "propagating β must be positive (+z direction)");
}

/// **Evanescent β on a real waveguide cross-section** (issue #254):
/// pick a frequency between the TE₁₀ and TE₂₀ cutoffs of a × b =
/// 2 × 1 — TE₁₀ propagates and TE₂₀ is evanescent. Verify the
/// outgoing-wave sign convention holds on a genuine modal solve
/// (not just the analytic `beta_complex` formula).
#[test]
fn evanescent_beta_below_te20_cutoff_outgoing() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    // TE₁₀ cutoff = π/a ≈ 1.5708; TE₂₀ cutoff = 2π/a ≈ 3.1416.
    // Pick ω = 2.0 (with c = 1): TE₁₀ propagates, TE₂₀ evanescent.
    let omega = 2.0_f64;
    let c = 1.0_f64;
    let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
    assert_eq!(modes.len(), 2, "expected K=2 modes");

    // mode[0] = TE₁₀ (lowest cutoff): propagating at ω = 2.
    let m0 = &modes[0];
    let beta0 = m0.beta_complex(omega, c);
    eprintln!(
        "TE₁₀-like: k_c = {:.4}, β(ω=2) = {} + {}j",
        m0.k_c, beta0.re, beta0.im
    );
    assert!(
        m0.k_c < omega,
        "mode[0] k_c {} should be below ω = {} (propagating)",
        m0.k_c,
        omega
    );
    assert!(beta0.im == 0.0, "TE₁₀ β must be real (propagating)");
    assert!(beta0.re > 0.0, "TE₁₀ β must be positive real");

    // mode[1] = TE₂₀ (next cutoff): evanescent at ω = 2.
    let m1 = &modes[1];
    let beta1 = m1.beta_complex(omega, c);
    eprintln!(
        "TE₂₀-like: k_c = {:.4}, β(ω=2) = {} + {}j",
        m1.k_c, beta1.re, beta1.im
    );
    assert!(
        m1.k_c > omega,
        "mode[1] k_c {} should be above ω = {} (evanescent)",
        m1.k_c,
        omega
    );
    assert!(
        beta1.re == 0.0,
        "TE₂₀ β must be pure imaginary (evanescent)"
    );
    assert!(
        beta1.im < 0.0,
        "TE₂₀ β must have Im < 0 (outgoing-wave branch); got Im = {}",
        beta1.im
    );
}

/// **Set-wise M-orthonormality** (issue #254): for `K = 2` returned
/// modes, verify `e_iᵀ M e_j = δ_ij` to f64 noise. Lanczos in the
/// M-inner product (with full reorthogonalization) gives this for
/// free; this test pins the property so a future solver change
/// can't silently regress.
#[test]
fn multi_mode_set_wise_m_orthonormal_k2() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
    assert_eq!(modes.len(), 2);

    // Reassemble the global mass matrix to test eᵀ M e in the
    // **full-edge** representation (which is the convention
    // WaveguideModeProfile uses).
    let (_k, m_dense) = assemble_2d_nedelec(&mesh);
    let n_edges = m_dense.nrows();
    assert_eq!(modes[0].e_edges.len(), n_edges);

    // Compute G_ij = e_iᵀ M e_j for i, j ∈ {0, 1}.
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
    let g10 = dot_me(1, 0);
    let g11 = dot_me(1, 1);
    eprintln!(
        "set-wise M-Gram: G00 = {:.3e}, G01 = {:.3e}, G10 = {:.3e}, G11 = {:.3e}",
        g00, g01, g10, g11
    );

    let tol = 1e-12_f64;
    assert!((g00 - 1.0).abs() < tol, "mode[0]ᵀ M mode[0] = {} ≠ 1", g00);
    assert!((g11 - 1.0).abs() < tol, "mode[1]ᵀ M mode[1] = {} ≠ 1", g11);
    assert!(g01.abs() < tol, "mode[0]ᵀ M mode[1] = {} ≠ 0", g01);
    assert!(g10.abs() < tol, "mode[1]ᵀ M mode[0] = {} ≠ 0", g10);
}

/// **Reference-integral gauge convention holds across refinements**
/// (issue #300, replacing the issue-#262 argmax assertion).
///
/// Every returned eigenvector is rotated so its projection onto the
/// fixed reference field it selects is non-negative. Verified across
/// two mesh resolutions (`nx = 10` and `nx = 16`) on a `2 × 0.8`
/// cross-section: for **each** mode the *same* reference (lowest index
/// clearing the relative floor) is selected at both resolutions and
/// the resulting projection is positive at both — i.e. the gauge sign
/// is reproducible across meshes. PR #261's historical failure was
/// that `nx = 10 → nx = 16` flipped the raw Lanczos sign on the
/// x-antisymmetric TE₂₀ eigenvector (whose largest-magnitude DOF is
/// not mesh-stable); the reference-integral gauge fixes exactly that
/// case because TE₂₀ locks onto the `sin(2πx/a)` reference instead of
/// a single DOF.
///
/// The cross-section is deliberately **non-degenerate**: a `2 × 1`
/// guide makes TE₂₀ (`k_c = 2π/a`) and TE₀₁ (`k_c = π/b`) degenerate,
/// so the second eigenvector would be an arbitrary, genuinely
/// mesh-dependent mixture of the two — a case with no well-defined
/// per-mode gauge. `b = 0.8` separates them (`k_c` ≈ π vs 3.93).
#[test]
fn reference_gauge_convention_holds_across_refinements() {
    let (a, b) = (2.0_f64, 0.8_f64);
    // Reference fields mirroring those in `gauge_fix_eigenvector`.
    let refs: [fn(f64, f64) -> [f64; 2]; 6] = [
        |sx, _sy| [0.0, (std::f64::consts::PI * sx).sin()],
        |sx, _sy| [0.0, (2.0 * std::f64::consts::PI * sx).sin()],
        |_sx, sy| [(std::f64::consts::PI * sy).sin(), 0.0],
        |_sx, sy| [(2.0 * std::f64::consts::PI * sy).sin(), 0.0],
        |_sx, _sy| [1.0, 0.0],
        |_sx, _sy| [0.0, 1.0],
    ];
    // Returns (selected_reference_index, projection) for a mode.
    let select_ref = |mesh: &TriMesh, e: &[f64]| -> (usize, f64) {
        let edges = mesh.edges();
        let (mut xmin, mut ymin) = (f64::INFINITY, f64::INFINITY);
        let (mut xmax, mut ymax) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &mesh.nodes {
            xmin = xmin.min(p[0]);
            ymin = ymin.min(p[1]);
            xmax = xmax.max(p[0]);
            ymax = ymax.max(p[1]);
        }
        let lx = (xmax - xmin).max(f64::EPSILON);
        let ly = (ymax - ymin).max(f64::EPSILON);
        let e_scale = e
            .iter()
            .fold(0.0, |a, &x| a + x * x)
            .sqrt()
            .max(f64::EPSILON);
        for (ri, f) in refs.iter().enumerate() {
            let mut proj = 0.0;
            let mut rscale = 0.0;
            for (i, &ei) in e.iter().enumerate() {
                if ei == 0.0 {
                    continue;
                }
                let [ea, eb] = edges[i];
                let pa = mesh.nodes[ea as usize];
                let pb = mesh.nodes[eb as usize];
                let t = [pb[0] - pa[0], pb[1] - pa[1]];
                let sx = (0.5 * (pa[0] + pb[0]) - xmin) / lx;
                let sy = (0.5 * (pa[1] + pb[1]) - ymin) / ly;
                let fv = f(sx, sy);
                let r = fv[0] * t[0] + fv[1] * t[1];
                proj += ei * r;
                rscale += r * r;
            }
            let floor = 1e-6 * e_scale * rscale.sqrt();
            if proj.abs() > floor {
                return (ri, proj);
            }
        }
        (usize::MAX, 0.0) // fell through to argmax fallback
    };

    let mut selected_by_mesh: Vec<Vec<usize>> = Vec::new();
    for &nx in &[10usize, 16usize] {
        let ny = nx / 2;
        let mesh = rect_tri_mesh(nx, ny, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
        assert_eq!(modes.len(), 2);
        let mut sel = Vec::new();
        for (i, mode) in modes.iter().enumerate() {
            let (ri, proj) = select_ref(&mesh, &mode.e_edges);
            eprintln!("nx={nx}: mode[{i}] selected reference {ri}, projection = {proj:+.6e}");
            assert_ne!(
                ri,
                usize::MAX,
                "nx={nx} mode[{i}] matched no reference (fell through to argmax \
                 fallback) — gauge is not mesh-stable for this mode"
            );
            assert!(
                proj > 0.0,
                "reference-gauge violated: nx={nx} mode[{i}] projection onto its \
                 selected reference {ri} is {proj:+.6e}, must be > 0"
            );
            sel.push(ri);
        }
        selected_by_mesh.push(sel);
    }
    // The same reference must be selected for each mode at both
    // resolutions (this is what makes the gauge sign cross-mesh
    // reproducible).
    assert_eq!(
        selected_by_mesh[0], selected_by_mesh[1],
        "reference selection differed across meshes ({:?} vs {:?}); gauge sign \
         would not be cross-mesh reproducible",
        selected_by_mesh[0], selected_by_mesh[1]
    );
}

/// **Reference-integral gauge — M-orthonormality preserved** (issue
/// #300). The gauge rotation is a unit-modulus (±1) scalar per mode,
/// so it must leave the set-wise Gram `e_iᵀ M e_j = δ_ij` exactly
/// intact. This is the norm/orthogonality invariant the issue
/// requires unit-testing.
#[test]
fn reference_gauge_preserves_orthonormality() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    let modes = solve_rect_waveguide_modes(&mesh, a, b, 3).expect("multi-mode solve K=3");
    assert_eq!(modes.len(), 3);

    let (_k, m_dense) = assemble_2d_nedelec(&mesh);
    let n_edges = m_dense.nrows();

    let dot_me = |i: usize, j: usize| -> f64 {
        let mut acc = 0.0_f64;
        for p in 0..n_edges {
            for q in 0..n_edges {
                acc += modes[i].e_edges[p] * m_dense[(p, q)] * modes[j].e_edges[q];
            }
        }
        acc
    };

    let tol = 1e-12_f64;
    for i in 0..modes.len() {
        for j in 0..modes.len() {
            let g = dot_me(i, j);
            let expect = if i == j { 1.0 } else { 0.0 };
            assert!(
                (g - expect).abs() < tol,
                "gauge broke M-orthonormality: G[{i}][{j}] = {g} ≠ {expect}"
            );
        }
    }
}

/// **Reference-integral gauge helper unit test** (issue #300):
/// verifies the in-place behaviour of [`gauge_fix_eigenvector`] on a
/// tiny explicit mesh.
///
/// 1. A vector with positive reference projection is left unchanged.
/// 2. Its negation is flipped back (the gauge pins proj ≥ 0).
/// 3. The flip is norm-preserving (Σ eᵢ² unchanged).
/// 4. An all-zero vector is a benign `Ok(())` no-op (no sign to pin,
///    so no cross-mesh-unstable argmax to guard against — issue #349).
#[test]
fn gauge_fix_eigenvector_unit() {
    // 1×1 single-quad mesh → 5 edges (4 boundary + 1 diagonal).
    let mesh = rect_tri_mesh(1, 1, 1.0, 1.0);
    let edges = mesh.edges();
    let n = edges.len();

    // Construct a synthetic field whose uniform-x projection is
    // positive: put a positive weight on the horizontal bottom edge
    // (nodes 0→1, tangent +x).
    let mut v = vec![0.0_f64; n];
    // Find the bottom horizontal edge (y=0 on both endpoints).
    let bottom = edges
        .iter()
        .position(|e| mesh.nodes[e[0] as usize][1] == 0.0 && mesh.nodes[e[1] as usize][1] == 0.0)
        .expect("a horizontal bottom edge exists");
    v[bottom] = 1.0;
    let norm0 = v.iter().map(|x| x * x).sum::<f64>();

    // Already-positive projection → unchanged.
    let mut v_pos = v.clone();
    gauge_fix_eigenvector(&mesh, &edges, &mut v_pos, 0).expect("positive proj is gaugable");
    assert_eq!(
        v_pos, v,
        "positive-projection vector must be left unchanged"
    );

    // Negated → flipped back to positive projection.
    let mut v_neg: Vec<f64> = v.iter().map(|x| -x).collect();
    gauge_fix_eigenvector(&mesh, &edges, &mut v_neg, 0).expect("negated proj is gaugable");
    assert_eq!(
        v_neg, v,
        "negative-projection vector must be flipped to match"
    );

    // Norm preserved through the flip.
    let norm_after = v_neg.iter().map(|x| x * x).sum::<f64>();
    assert!(
        (norm_after - norm0).abs() < 1e-15,
        "gauge flip must preserve the eigenvector norm: {norm_after} ≠ {norm0}"
    );

    // All-zero vector → benign no-op (no sign to pin), stays zero.
    let mut z = vec![0.0_f64; n];
    gauge_fix_eigenvector(&mesh, &edges, &mut z, 0).expect("all-zero vector is a no-op");
    assert!(z.iter().all(|&x| x == 0.0));
}

/// **Loud guard for ungaugable modes** (issue #349): a non-zero
/// eigenvector that is orthogonal to *every* reference field in the
/// fixed basis must produce [`EigenError::UngaugableMode`], NOT a
/// silent fall-through to the cross-mesh-unstable argmax pin.
///
/// We construct the orthogonal vector directly on a tiny synthetic
/// edge set so the orthogonality is exact and analytic rather than
/// mesh-dependent. Two horizontal edges of equal length, both sitting
/// on the box's bottom rail (`sy = 0`), carry equal-and-opposite
/// weights `[+1, −1]`. Then for that vector:
///
/// - the y-directed `sin(mπsx)` references (1, 2) dot a horizontal
///   tangent to exactly 0 edge-by-edge;
/// - the x-directed `sin(nπsy)` references (3, 4) carry `sin(nπ·0)=0`
///   at both midpoints, so they vanish edge-by-edge;
/// - the uniform-x catch-all (5) gives `+|t| − |t| = 0` by the
///   equal-and-opposite cancellation (both edges same length);
/// - the uniform-y catch-all (6) dots a horizontal tangent to 0.
///
/// So every projection is exactly zero and the gauge must refuse to
/// pin the sign, returning the loud error with the offending mode
/// index. This is the test that proves the #300 silent-argmax hazard
/// can no longer be hit in production.
#[test]
fn gauge_fix_eigenvector_loud_on_ungaugable_mode() {
    // Bounding box [0,1]×[0,1]: nodes 2,3 force the box height so the
    // two weighted edges (0→1 and 4→5) sit at sy = 0 within it.
    let mesh = TriMesh {
        nodes: vec![
            [0.0, 0.0], // 0  ┐ first horizontal edge (0→1)
            [0.4, 0.0], // 1  ┘
            [0.0, 1.0], // 2  box-height anchor (top-left)
            [1.0, 1.0], // 3  box-width / height anchor (top-right)
            [0.6, 0.0], // 4  ┐ second horizontal edge (4→5)
            [1.0, 0.0], // 5  ┘  same length 0.4 as the first
        ],
        tris: vec![],
    };
    // Hand-built edge table (a < b by convention). Only the two
    // horizontal bottom edges carry weight; the rest are zeros so they
    // do not contribute to any projection.
    let edges = vec![[0u32, 1], [2, 3], [4, 5]];
    // Equal-and-opposite weights on the two equal-length horizontal
    // edges; zero on the spacer edge.
    let mut v = vec![1.0_f64, 0.0, -1.0];

    let err = gauge_fix_eigenvector(&mesh, &edges, &mut v, 7)
        .expect_err("a mode orthogonal to every reference must be loudly ungaugable");
    match err {
        EigenError::UngaugableMode {
            mode,
            best_rel_proj,
        } => {
            assert_eq!(mode, 7, "error must carry the offending mode index");
            assert!(
                best_rel_proj < 1e-6,
                "best relative projection {best_rel_proj:.3e} must be below the \
                 gauge floor for a genuinely orthogonal mode"
            );
        }
        other => panic!("expected UngaugableMode, got {other:?}"),
    }

    // The vector must be left untouched on the loud path (no partial
    // mutation): the caller is responsible for handling the error.
    assert_eq!(
        v,
        vec![1.0, 0.0, -1.0],
        "ungaugable vector must not be mutated on the error path"
    );
}

/// **Sign pin helper unit test**: verifies the in-place flip
/// behaviour of [`pin_eigenvector_sign`] directly on synthetic
/// inputs. Three cases:
///
/// 1. Largest-magnitude entry is already positive → no flip.
/// 2. Largest-magnitude entry is negative → vector negated.
/// 3. Tie at maximum magnitude between two entries with opposite
///    signs → behaviour follows `position_max_by` tie-breaking
///    (lowest index wins), so the sign at the lowest-index tied
///    entry is what matters.
#[test]
fn pin_eigenvector_sign_unit() {
    // Case 1: already positive at argmax → no change.
    let mut v = vec![0.1, -0.3, 0.7, -0.2];
    let orig = v.clone();
    pin_eigenvector_sign(&mut v);
    assert_eq!(v, orig, "no flip when argmax is already positive");

    // Case 2: argmax negative → flip.
    let mut v = vec![0.1, -0.7, 0.3, -0.2];
    pin_eigenvector_sign(&mut v);
    assert_eq!(
        v,
        vec![-0.1, 0.7, -0.3, 0.2],
        "flip when argmax is negative"
    );

    // Case 3: empty input → no panic.
    let mut v: Vec<f64> = vec![];
    pin_eigenvector_sign(&mut v);
    assert!(v.is_empty());

    // Case 4: all zeros → max returns the first (val = 0.0 which
    // is not < 0.0), so no flip; result still all zeros.
    let mut v = vec![0.0; 5];
    pin_eigenvector_sign(&mut v);
    assert_eq!(v, vec![0.0; 5]);
}

#[test]
fn rect_waveguide_cutoff_te10_te20() {
    let a = 22.86e-3; // WR-90 inner width.
    let b = 10.16e-3;
    let pi = std::f64::consts::PI;
    assert!((rect_waveguide_cutoff(1, 0, a, b) - pi / a).abs() < 1e-12);
    assert!((rect_waveguide_cutoff(2, 0, a, b) - 2.0 * pi / a).abs() < 1e-12);
    assert!((rect_waveguide_cutoff(0, 1, a, b) - pi / b).abs() < 1e-12);
}

/// Whitney/Nédélec rectangular-waveguide cutoff regression: the
/// lowest four eigenmodes of the curl-curl pencil on a 16×8 mesh
/// pair to TE₁₀, TE₂₀, TE₀₁, and TM₁₁ within a few percent. This
/// is the load-bearing Phase-1 acceptance test (#235) — once it
/// passes the eigensolver is consuming a 2-D triangle mesh, doing
/// the de-Rham nullspace filter, and producing modal cutoffs that
/// match the analytic oracle.
#[test]
fn rect_waveguide_te10_matches_analytic() {
    // WR-90-ish dimensions in arbitrary length units.
    let a = 2.0;
    let b = 1.0;
    let mesh = rect_tri_mesh(16, 8, a, b);
    let modes = solve_rect_waveguide_modes(&mesh, a, b, 5).expect("modal eigensolve");

    // TE₁₀ at k_c = π/a is the dominant mode.
    let pi = std::f64::consts::PI;
    let kc_te10 = pi / a;
    let rel_err_te10 = (modes[0].k_c - kc_te10).abs() / kc_te10;
    eprintln!(
        "modal cutoffs: TE10 fem k_c = {:.4} (analytic {:.4}, rel err {:.2}%)",
        modes[0].k_c,
        kc_te10,
        100.0 * rel_err_te10
    );
    assert!(
        rel_err_te10 < 0.03,
        "TE10 cutoff disagreement: fem = {:.4}, analytic = {:.4} ({:.2}%)",
        modes[0].k_c,
        kc_te10,
        100.0 * rel_err_te10
    );

    // The remaining four FEM cutoffs should pair to the lowest
    // analytic catalog roots within 5 %. We pair by closest k_c.
    let catalog: Vec<(u32, u32, f64)> = (0..=3)
        .flat_map(|m| (0..=3).map(move |n| (m as u32, n as u32)))
        .filter(|&(m, n)| !(m == 0 && n == 0))
        .map(|(m, n)| (m, n, rect_waveguide_cutoff(m, n, a, b)))
        .collect();

    for (i, mode) in modes.iter().enumerate().take(4) {
        let closest = catalog
            .iter()
            .min_by(|a, b| {
                (a.2 - mode.k_c)
                    .abs()
                    .partial_cmp(&(b.2 - mode.k_c).abs())
                    .unwrap()
            })
            .unwrap();
        let rel_err = (mode.k_c - closest.2).abs() / closest.2;
        eprintln!(
            "  mode[{i}]: k_c = {:.4}  →  TE/TM ({},{}) analytic k_c = {:.4} ({:.2}%)",
            mode.k_c,
            closest.0,
            closest.1,
            closest.2,
            100.0 * rel_err
        );
        assert!(
            rel_err < 0.05,
            "mode[{i}] k_c = {:.4} fails to pair to any (m,n) within 5 %",
            mode.k_c
        );
    }
}

// --- Phase-1A: per-element ε(x,y) in the 2-D Nédélec assembly ---

#[test]
fn epsilon_assembly_uniform_one_matches_homogeneous_bit_for_bit() {
    // Non-regression guard: uniform ε_r = 1 must reproduce the
    // homogeneous assembly exactly (IEEE-754 bit-for-bit), since the
    // only added arithmetic is the identity `1.0 * m_local[i][j]`.
    let mesh = rect_tri_mesh(5, 3, 2.0, 1.0);
    let (k_ref, m_ref) = assemble_2d_nedelec(&mesh);

    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let (k_eps, m_eps) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);

    assert_eq!(k_eps.nrows(), k_ref.nrows());
    assert_eq!(m_eps.nrows(), m_ref.nrows());
    for i in 0..k_ref.nrows() {
        for j in 0..k_ref.ncols() {
            // Bit-for-bit equality via the raw f64 bit patterns.
            assert_eq!(
                k_eps[(i, j)].to_bits(),
                k_ref[(i, j)].to_bits(),
                "K differs at ({i},{j})"
            );
            assert_eq!(
                m_eps[(i, j)].to_bits(),
                m_ref[(i, j)].to_bits(),
                "M differs at ({i},{j}) for uniform ε_r = 1"
            );
        }
    }
}

#[test]
fn epsilon_assembly_two_region_mass_scales_high_eps_region_exactly() {
    // Two horizontal regions: bottom half (y < H/2) is "core" with
    // ε_r = EPS_HI, the top half is "cladding" with ε_r = 1. The
    // curl-curl K must be identical to the homogeneous case, and the
    // mass M must equal the homogeneous M scaled element-wise — so
    // the assembled M entries that receive *only* high-ε triangles
    // are exactly EPS_HI× their homogeneous values, while entries
    // touched only by the ε = 1 region are unchanged.
    const EPS_HI: f64 = 12.0;
    let (nx, ny) = (4, 4);
    let (w, h) = (1.0, 1.0);
    let mesh = rect_tri_mesh(nx, ny, w, h);

    let (k_ref, m_ref) = assemble_2d_nedelec(&mesh);

    // Region tag per triangle from its centroid: 1 = core, 0 = clad.
    let region_tags: Vec<i32> = mesh
        .tris
        .iter()
        .map(|t| {
            let yc = (mesh.nodes[t[0] as usize][1]
                + mesh.nodes[t[1] as usize][1]
                + mesh.nodes[t[2] as usize][1])
                / 3.0;
            if yc < h / 2.0 { 1 } else { 0 }
        })
        .collect();
    let eps_r = epsilon_r_from_region_tags(&region_tags, |tag| if tag == 1 { EPS_HI } else { 1.0 });

    let (k_eps, m_eps) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_r);

    // 1. K is ε-independent: identical bit-for-bit to homogeneous.
    for i in 0..k_ref.nrows() {
        for j in 0..k_ref.ncols() {
            assert_eq!(
                k_eps[(i, j)].to_bits(),
                k_ref[(i, j)].to_bits(),
                "curl-curl K must not depend on ε at ({i},{j})"
            );
        }
    }

    // 2. The two regions actually contribute (sanity: the tags split
    //    the mesh into nonempty halves).
    let n_core = region_tags.iter().filter(|&&t| t == 1).count();
    assert!(
        n_core > 0 && n_core < mesh.n_tris(),
        "two-region split is degenerate"
    );

    // 3. Independently reassemble M with the local mass scaled by the
    //    triangle's ε, and confirm it matches the ε-aware path
    //    bit-for-bit (i.e. ε weights exactly the per-element mass).
    let edges = mesh.edges();
    let tri_edges = mesh.tri_edges();
    let mut m_expected = Mat::<f64>::zeros(edges.len(), edges.len());
    for ((tri, row), &eps) in mesh.tris.iter().zip(tri_edges.iter()).zip(eps_r.iter()) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_, m_local, _) = tri_nedelec_local(&coords);
        for i in 0..3 {
            let (gi, si) = row[i];
            for j in 0..3 {
                let (gj, sj) = row[j];
                let s = (si as f64) * (sj as f64);
                m_expected[(gi as usize, gj as usize)] += s * eps * m_local[i][j];
            }
        }
    }
    for i in 0..m_ref.nrows() {
        for j in 0..m_ref.ncols() {
            assert_eq!(
                m_eps[(i, j)].to_bits(),
                m_expected[(i, j)].to_bits(),
                "ε-weighted M mismatch at ({i},{j})"
            );
        }
    }

    // 4. The high-ε region strictly increases the mass: the total
    //    mass-matrix trace grows, and at least one diagonal entry is
    //    exactly EPS_HI× its homogeneous value (an edge interior to
    //    the core, touched only by core triangles).
    let trace_ref: f64 = (0..m_ref.nrows()).map(|i| m_ref[(i, i)]).sum();
    let trace_eps: f64 = (0..m_eps.nrows()).map(|i| m_eps[(i, i)]).sum();
    assert!(
        trace_eps > trace_ref,
        "high-ε region must increase total mass: {trace_eps} !> {trace_ref}"
    );

    let scaled_exactly = (0..m_ref.nrows()).any(|i| {
        let r = m_ref[(i, i)];
        r != 0.0 && (m_eps[(i, i)] - EPS_HI * r).abs() <= 1e-12 * (EPS_HI * r).abs()
    });
    assert!(
        scaled_exactly,
        "expected at least one core-interior edge scaled exactly by EPS_HI"
    );
}

#[test]
#[should_panic(expected = "must equal the triangle count")]
fn epsilon_assembly_rejects_length_mismatch() {
    let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
    let eps_r = vec![1.0; mesh.n_tris() + 1];
    let _ = assemble_2d_nedelec_with_epsilon(&mesh, &eps_r);
}

#[test]
#[should_panic(expected = "invalid ε_r")]
fn region_tag_helper_rejects_nonpositive_epsilon() {
    let tags = [0, 1, 0];
    let _ = epsilon_r_from_region_tags(&tags, |t| if t == 1 { -1.0 } else { 1.0 });
}

// --- Phase-1B: dielectric n_eff solve + slab analytic oracle ---

/// The slab oracle returns an `n_eff` strictly inside `(n_clad,
/// n_core)` and satisfies the dispersion `tan(κ a) = γ/κ` it solves.
#[test]
fn slab_oracle_in_window_and_satisfies_dispersion() {
    // SOI-ish slab: Si core, SiO₂ cladding, λ = 1.55 µm.
    let n_core = 3.45;
    let n_clad = 1.45;
    let lambda = 1.55; // µm
    let k0 = 2.0 * std::f64::consts::PI / lambda;
    let d = 0.22; // 220 nm core thickness
    let n_eff = slab_te0_neff(n_core, n_clad, d, k0);
    eprintln!("slab oracle: n_eff = {n_eff:.6} (n_clad={n_clad}, n_core={n_core})");
    assert!(
        n_eff > n_clad && n_eff < n_core,
        "n_eff {n_eff} not in (n_clad, n_core)"
    );
    // Residual of the fundamental even-mode dispersion at the root.
    let a = 0.5 * d;
    let kappa = k0 * (n_core * n_core - n_eff * n_eff).sqrt();
    let gamma = k0 * (n_eff * n_eff - n_clad * n_clad).sqrt();
    let res = (kappa * a).tan() - gamma / kappa;
    assert!(
        res.abs() < 1e-6,
        "dispersion residual tan(κa)-γ/κ = {res} not ~0"
    );
}

/// Build a slab-like fixture: a rectangle `[0,W] × [0,H]` invariant in
/// x, with a high-index **core stripe** of full thickness `d` centred
/// at `y = H/2`, clad above and below. Triangles are tagged by
/// centroid: tag 1 (core) if `|y_c − H/2| < d/2`, else tag 0 (clad).
fn slab_fixture(
    nx: usize,
    ny: usize,
    w: f64,
    h: f64,
    d: f64,
    eps_core: f64,
    eps_clad: f64,
) -> (TriMesh, Vec<f64>, Vec<bool>) {
    let mesh = rect_tri_mesh(nx, ny, w, h);
    let region_tags: Vec<i32> = mesh
        .tris
        .iter()
        .map(|t| {
            let yc = (mesh.nodes[t[0] as usize][1]
                + mesh.nodes[t[1] as usize][1]
                + mesh.nodes[t[2] as usize][1])
                / 3.0;
            if (yc - 0.5 * h).abs() < 0.5 * d { 1 } else { 0 }
        })
        .collect();
    let eps_r =
        epsilon_r_from_region_tags(
            &region_tags,
            |tag| {
                if tag == 1 { eps_core } else { eps_clad }
            },
        );
    let (_edges, interior) = rect_pec_interior_edges(&mesh, w, h);
    (mesh, eps_r, interior)
}

/// **Slab fundamental-mode n_eff acceptance test** (Epic #303 Phase
/// 1B, issue #305): the FEM dielectric solve on a wide slab-like
/// fixture must reproduce the 1-D analytic slab oracle within ≤1 % on
/// a converged mesh.
///
/// The PEC box is placed far above/below the core so the bound mode
/// has decayed to the wall (the truncation is immaterial). The core
/// is one element thick in the invariant (x) direction is *not*
/// required — we keep the mesh wide in x and resolve the y-profile.
#[test]
fn slab_fundamental_neff_matches_oracle() {
    let n_core = 3.45_f64;
    let n_clad = 1.45_f64;
    let eps_core = n_core * n_core;
    let eps_clad = n_clad * n_clad;
    let lambda = 1.55_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;
    let d = 0.22_f64; // core thickness

    // Wide computational window: cladding extends many decay lengths
    // above/below the core so PEC truncation doesn't perturb the
    // bound mode. W small (invariant direction); H tall.
    let w = 0.20_f64;
    let h = 4.0_f64; // many µm of cladding each side
    // Keep elements near-isotropic to suppress spurious edge-element
    // modes (anisotropic slivers from a tall thin domain pollute the
    // spectrum). Element size ≈ w/nx ≈ h/ny.
    let nx = 4;
    let ny = 80;
    let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);

    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
    assert!(!modes.is_empty(), "expected at least the fundamental mode");
    // All returned modes must be flagged guided and lie in the window.
    for m in &modes {
        assert!(m.guided, "returned mode must be guided");
        assert!(
            m.n_eff > n_clad && m.n_eff < n_core,
            "n_eff {} outside (n_clad, n_core)",
            m.n_eff
        );
    }
    // Fundamental is first (largest n_eff).
    let n_eff_fem = modes[0].n_eff;
    let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);
    let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
    eprintln!(
        "slab fundamental: n_eff_fem = {n_eff_fem:.6}, n_eff_oracle = \
         {n_eff_oracle:.6}, rel err = {:.3}%",
        100.0 * rel_err
    );
    assert!(
        rel_err < 0.01,
        "slab fundamental n_eff disagreement: fem {n_eff_fem:.6} vs oracle \
         {n_eff_oracle:.6} ({:.3}% > 1%)",
        100.0 * rel_err
    );
}

/// **p=2 dielectric solve path** (Epic #318 Phase 2.5C): the
/// order-aware `solve_dielectric_modes2` assembles the p=2 pencil with
/// `assemble_2d_nedelec2_with_epsilon` + the p=2 interior-DOF mask and
/// returns a guided fundamental in the bound window. On a slab-like
/// fixture the p=2 fundamental `n_eff` matches the 1-D slab oracle, and
/// at equal mesh density it is **at least as accurate** as the p=1
/// solve (the higher-order element resolves the y-profile better). Mesh
/// kept modest because the p=2 dense assembly is ~4× the p=1 system.
#[test]
fn p2_dielectric_solve_returns_guided_fundamental() {
    let n_core = 3.45_f64; // silicon-ish
    let n_clad = 1.45_f64; // oxide
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let d = 0.30_f64;
    let (w, h) = (0.20_f64, 3.0_f64);
    let (nx, ny) = (4usize, 24usize);
    let (mesh, eps_r, _interior_p1) =
        slab_fixture(nx, ny, w, h, d, n_core * n_core, n_clad * n_clad);

    // p=2 interior-DOF mask + p=2 solve.
    let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);
    let modes2 =
        solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1).expect("p=2 dielectric solve");
    assert!(
        !modes2.is_empty(),
        "p=2 solve must return at least the fundamental guided mode"
    );
    let m2 = &modes2[0];
    assert!(m2.guided, "p=2 fundamental must be flagged guided");
    assert!(
        m2.n_eff > n_clad && m2.n_eff < n_core,
        "p=2 n_eff {} outside (n_clad, n_core)",
        m2.n_eff
    );
    // Field profile is in the p=2 DOF layout.
    assert_eq!(m2.e_edges.len(), n_dof_2d_nedelec2(&mesh));

    // Compare to the p=1 solve on the identical mesh against the slab
    // oracle: the p=2 fundamental n_eff is at least as accurate.
    let (_e, interior_p1) = rect_pec_interior_edges(&mesh, w, h);
    let modes1 =
        solve_dielectric_modes(&mesh, &eps_r, &interior_p1, k0, 1).expect("p=1 dielectric solve");
    assert!(!modes1.is_empty());
    let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);
    let err1 = (modes1[0].n_eff - n_eff_oracle).abs();
    let err2 = (m2.n_eff - n_eff_oracle).abs();
    eprintln!(
        "p=2 dielectric: n_eff p1 = {:.6} (err {:.2e}), p2 = {:.6} (err {:.2e}), \
         oracle = {n_eff_oracle:.6}",
        modes1[0].n_eff, err1, m2.n_eff, err2
    );
    assert!(
        err2 <= err1 + 1e-9,
        "p=2 fundamental n_eff error {err2:.3e} worse than p=1 {err1:.3e}"
    );
}

/// **Contrast-scaled p=2 curl floor** (issue #791). The floor must sit
/// well below the exact Rayleigh bound `r < (ε_max − ε_min)/ε_max` on the
/// curl ratio of any in-window eigenpair (otherwise no guided mode of a
/// weakly-guiding fiber can pass), keep the calibrated `3e-2` for
/// Si/SiO₂, and stay above the gradient-nullspace noise for any input.
#[test]
fn p2_curl_floor_scales_with_index_contrast() {
    // SMF-28: exact bound = 7.8e-3, below the old fixed 3e-2 floor.
    let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
    let bound = (e_core - e_clad) / e_core;
    let floor = physical_curl_floor_p2(e_core, e_clad);
    assert!(bound < HIGH_CONTRAST_CURL_FLOOR, "SMF-28 bound {bound:.3e}");
    assert!(
        (floor - GUIDED_CURL_FLOOR_FRACTION * (e_core - e_clad) / e_clad).abs() < 1e-18,
        "SMF-28 floor {floor:.3e} must be the scaled contrast"
    );
    // The measured SMF-28 guided fundamental (r = 7.2e-4, issue #791)
    // passes with margin; the old fixed floor rejected it.
    assert!(7.2e-4 > 5.0 * floor && 7.2e-4 < HIGH_CONTRAST_CURL_FLOOR);

    // Si/SiO₂ (contrast ≈ 4.7): exactly the calibrated high-contrast
    // floor, so the high-contrast floor is unchanged.
    for (n_core, n_clad) in [(3.45_f64, 1.45_f64), (3.48, 1.444)] {
        let f = physical_curl_floor_p2(n_core * n_core, n_clad * n_clad);
        assert_eq!(f, HIGH_CONTRAST_CURL_FLOOR, "Si/SiO2 ({n_core}, {n_clad})");
    }

    // Uniform ε (no contrast): the nullspace floor, never zero.
    assert_eq!(physical_curl_floor_p2(2.0, 2.0), physical_curl_floor_pml());

    // Monotone in the contrast, and always below 0.04 × the exact bound
    // (the `/ε_min` scaling never pushes the floor near the bound).
    let mut prev = 0.0;
    for i in 1..=200 {
        let e_core = 2.0 + 0.1 * f64::from(i);
        let f = physical_curl_floor_p2(e_core, 2.0);
        let exact_bound = (e_core - 2.0) / e_core;
        assert!(f >= prev, "floor must not decrease with contrast");
        assert!(
            f <= 0.04 * exact_bound + 1e-15,
            "floor {f:.3e} above 0.04 × exact bound {exact_bound:.3e}"
        );
        prev = f;
    }
}

/// **Rayleigh-identity rejection** (issue #791). Every eigenpair of
/// `A x = β² M₁ x` satisfies `r = 1 − n_eff²/⟨ε⟩_x`, so an in-window
/// candidate with `r ≥ (ε_max − ε_min)/ε_max`, or whose `δ` exceeds the
/// tolerance, is not an eigenpair and must be rejected. Injects the two
/// unconverged Ritz pairs the pre-#791 selection returned as
/// "fundamentals" (values measured at mesh (5,48)), next to the genuine
/// converged fundamentals of the same solves.
#[test]
fn p2_rayleigh_identity_rejects_unconverged_pairs() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let cand = |n_eff: f64, r: f64, eps_w: f64| RawDielectricCandidate {
        beta_sq: n_eff * n_eff * k0 * k0,
        curl_ratio: r,
        eps_weighted: eps_w,
        residual: 0.0,
        vector: Vec::new(),
    };
    // SMF-28.
    let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
    let genuine = cand(1.448_196_80, 7.2353e-4, 2.098_792);
    assert!(rayleigh_identity_violation(&genuine, k0) < 1e-6);
    assert!(rayleigh_consistent(&genuine, k0, e_core, e_clad));
    let old_pick = cand(1.445_372_57, 3.3259e-2, 2.090_577);
    // r above the exact bound: needs ⟨ε⟩ = n²/(1−r) > ε_core.
    assert!(old_pick.curl_ratio >= (e_core - e_clad) / e_core);
    assert!(1.445_372_57_f64.powi(2) / (1.0 - old_pick.curl_ratio) > e_core);
    assert!(!rayleigh_consistent(&old_pick, k0, e_core, e_clad));
    // An unconverged tail pair below the bound, caught by δ alone
    // (measured δ = 1.1e-5 against the threshold 7.8e-6).
    let tail = cand(1.446_870_79, 8.3127e-5, 2.093_586);
    assert!(tail.curl_ratio < (e_core - e_clad) / e_core);
    assert!(!rayleigh_consistent(&tail, k0, e_core, e_clad));

    // ~3 %-step fiber.
    let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
    let genuine = cand(1.470_557_30, 4.9671e-3, 2.173_334);
    assert!(rayleigh_consistent(&genuine, k0, e_core, e_clad));
    let old_pick = cand(1.468_777_68, 3.8441e-1, 2.153_019);
    assert!(1.468_777_68_f64.powi(2) / (1.0 - old_pick.curl_ratio) > e_core);
    assert!(!rayleigh_consistent(&old_pick, k0, e_core, e_clad));
}

/// **Intermediate contrast (SiN/SiO₂) and a constructed unconverged
/// vector** (issue #791). At contrast ≈ 0.9 the p=2 floor is ≈ 9e-3,
/// below the 3e-2 cap. On a 2-D-confined SiN strip the PEC p=2 solve
/// must return a fundamental just below the physical (vertical-slab)
/// index ceiling, whose curl ratio (≈ 2.5e-2) the old fixed 3e-2 floor
/// would have rejected, and which satisfies the Rayleigh identity. A
/// Ritz-like vector built by mixing two converged eigenvectors while
/// keeping one reported `β²` breaks the identity and must be rejected.
#[test]
fn p2_dielectric_solve_intermediate_contrast_sin_strip() {
    let n_core = 2.0_f64; // SiN
    let n_clad = 1.45_f64; // oxide
    let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (w, h) = (3.0_f64, 3.0_f64);
    let (w_core, d_core) = (1.2_f64, 0.6_f64);
    let (mesh, eps_r, _interior_p1) =
        strip_fixture((30, 30), (w, h), (w_core, d_core), (e_core, e_clad));
    let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);

    let floor = physical_curl_floor_p2(e_core, e_clad);
    assert!(
        floor > 5e-3 && floor < HIGH_CONTRAST_CURL_FLOOR,
        "SiN floor {floor:.3e} must be in the scaled (uncapped) regime"
    );

    let modes =
        solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1).expect("p=2 dielectric solve");
    assert!(!modes.is_empty(), "SiN slab must return its fundamental");
    let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
        .expect("a 2-D-confined strip yields a physical ceiling");
    let gap = ceiling - modes[0].n_eff;
    eprintln!(
        "SiN strip p=2: n_eff = {:.6}, physical ceiling = {ceiling:.6}, gap = {gap:.2e}, \
         floor = {floor:.3e}",
        modes[0].n_eff
    );
    // Measured n_eff 1.830185, 7.3e-3 below the ceiling 1.837505.
    assert!(
        gap > 0.0 && gap < 2e-2,
        "SiN fundamental {:.6} must sit just below the physical ceiling {ceiling:.6}",
        modes[0].n_eff
    );

    // Raw candidates: every returned (kept) pair obeys the identity; a
    // mixed vector with a stale β² does not.
    let cands = dielectric_raw_candidates_p2(&mesh, &eps_r, &dof_mask, k0, 16, Some(ceiling))
        .expect("raw p=2 candidates")
        .cands;
    let fund = cands
        .iter()
        .find(|c| (c.beta_sq - modes[0].beta_sq).abs() <= 1e-12 * modes[0].beta_sq)
        .expect("the returned fundamental is a raw candidate");
    assert!(
        rayleigh_identity_violation(fund, k0) < 1e-8,
        "converged fundamental δ = {:.3e}",
        rayleigh_identity_violation(fund, k0)
    );
    // The old fixed floor would have rejected this fundamental.
    assert!(fund.curl_ratio > floor && fund.curl_ratio < HIGH_CONTRAST_CURL_FLOOR);

    // Mix the fundamental with the lowest-β² converged candidate.
    let other = cands
        .iter()
        .filter(|c| rayleigh_identity_violation(c, k0) < 1e-8)
        .min_by(|a, b| a.beta_sq.partial_cmp(&b.beta_sq).unwrap())
        .expect("a second converged candidate");
    assert!(other.beta_sq < 0.99 * fund.beta_sq);
    let ops =
        assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &dof_mask).expect("interior operators");
    let mixed: Vec<f64> = fund
        .vector
        .iter()
        .zip(&other.vector)
        .map(|(a, b)| a + b)
        .collect();
    let xkx = sparse_quadratic_form(ops.k.as_ref(), &mixed);
    let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &mixed);
    let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &mixed);
    let fake = RawDielectricCandidate {
        beta_sq: fund.beta_sq,
        curl_ratio: xkx.abs() / (k0 * k0 * xmx),
        eps_weighted: xmx / xm1x,
        residual: f64::NAN,
        vector: mixed,
    };
    let delta = rayleigh_identity_violation(&fake, k0);
    eprintln!("mixed (unconverged) vector: δ = {delta:.3e}");
    assert!(
        !rayleigh_consistent(&fake, k0, e_core, e_clad),
        "a mixed vector with a stale β² (δ = {delta:.3e}) must be rejected"
    );
}

/// The PEC p=2 / p=1 fiber fixture of the #449/#791 audit: SMF-28-like
/// or ~3 %-step core of radius `a` (µm) in a `6a` PEC disk, mesh (5,48).
/// Returns `(mesh, region_tags, eps_r)`.
fn fiber_fixture(n_core: f64, n_clad: f64, a: f64) -> (TriMesh, Vec<i32>, Vec<f64>) {
    let (mesh, tags) = disk_tri_mesh(a, 6.0 * a, 5, 48);
    let eps_r = epsilon_r_from_region_tags(&tags, |t| {
        if t == 1 {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    (mesh, tags, eps_r)
}

/// **Residual check catches the leaking SMF-28 tail pair** (issue #798).
/// With the historical budget (`max_iters = n_request + 8`, 16 pairs) the
/// SMF-28 p=2 window contains unconverged in-window pairs with true
/// residuals far above `DIELECTRIC_RESIDUAL_TOL`. The #791 Rayleigh check
/// cannot be relied on to catch them, because its violation `δ` is second
/// order in the residual. Measured on macOS/aarch64: one pair at
/// `n_eff ≈ 1.446871` (residual `3.5e-3`, `δ = 1.1e-5`) cleared the
/// threshold (`7.8e-6`) by only 1.4×, and the smallest-`δ` pair (residual
/// `3.4e-4`, `δ = 1.0e-7`) passes it outright. The test picks the pair by
/// these properties, not by its Ritz value, which is not reproducible
/// across platforms. The checked solve returns only converged pairs, so
/// none of them can reach the filters, and every returned candidate
/// satisfies the Rayleigh identity to round-off.
#[test]
fn residual_check_withholds_smf28_tail_pair() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
    let (mesh, _tags, eps_r) = fiber_fixture(1.4504, 1.4447, 4.1);
    let dof_mask = disk_pec_interior_dofs2(&mesh, 6.0 * 4.1);
    let ceiling = physical_index_ceiling(&mesh, &eps_r, k0);

    // Re-run the historical, unchecked first pass on the same pencil.
    let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &dof_mask).unwrap();
    let k0_sq = k0 * k0;
    let a = sparse_pencil_a(ops.k.as_ref(), ops.m_eps.as_ref(), k0_sq).unwrap();
    let n_core = e_core.sqrt();
    let target = match ceiling {
        Some(c) if c < n_core => c * c * k0_sq,
        _ => e_core * k0_sq,
    };
    let sigma = target * (1.0 - 1e-3);
    let old = SparseShiftInvertLanczos {
        sigma,
        max_iters: 24,
        tol: 1e-9,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    }
    .smallest_eigenpairs(a.as_ref(), ops.m1.as_ref(), 16)
    .unwrap();
    let spmv = |mat: SparseColMatRef<'_, usize, f64>, x: &[f64]| -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for (j, &xj) in x.iter().enumerate() {
            let range = mat.col_ptr()[j]..mat.col_ptr()[j + 1];
            for (&i, &v) in mat.row_idx()[range.clone()].iter().zip(&mat.val()[range]) {
                y[i] += v * xj;
            }
        }
        y
    };
    // (residual, δ) of one historical pair.
    let diagnose = |pair: &EigenPair| -> (f64, f64) {
        let kx = spmv(a.as_ref(), &pair.vector);
        let mx = spmv(ops.m1.as_ref(), &pair.vector);
        let r2: f64 = kx
            .iter()
            .zip(&mx)
            .map(|(k, m)| (k - pair.lambda * m).powi(2))
            .sum();
        let m2: f64 = mx.iter().map(|m| m * m).sum();
        let residual = r2.sqrt() / (pair.lambda * m2.sqrt());
        let xkx = sparse_quadratic_form(ops.k.as_ref(), &pair.vector);
        let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &pair.vector);
        let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &pair.vector);
        let cand = RawDielectricCandidate {
            beta_sq: pair.lambda,
            curl_ratio: xkx / (k0_sq * xmx),
            eps_weighted: xmx / xm1x,
            residual,
            vector: Vec::new(),
        };
        (residual, rayleigh_identity_violation(&cand, k0))
    };
    // The leaking tail pair: an unconverged pair inside the guided window
    // whose Rayleigh violation is the smallest, i.e. the one closest to
    // slipping past the #791 check. It is selected by these properties,
    // not by its Ritz value: an unconverged Ritz value (residual ~ 1e-3)
    // is not reproducible across platforms (measured n_eff ≈ 1.446871 on
    // macOS/aarch64; it differs on Linux x86_64 CI).
    let in_guided = |lambda: f64| e_clad * k0_sq < lambda && lambda < e_core * k0_sq;
    let unconverged: Vec<(f64, f64, f64)> = old
        .iter()
        .filter(|p| in_guided(p.lambda))
        .map(|p| {
            let (residual, delta) = diagnose(p);
            (p.lambda, residual, delta)
        })
        .filter(|&(_, residual, _)| residual > 1e4 * DIELECTRIC_RESIDUAL_TOL)
        .collect();
    let &(tail_lambda, residual, delta) = unconverged
        .iter()
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .expect("the historical window holds an unconverged in-window tail pair");
    eprintln!(
        "SMF-28 tail pair: n_eff = {:.8}, residual = {residual:.3e}, δ = {delta:.3e} \
         ({} unconverged in-window pairs)",
        tail_lambda.sqrt() / k0,
        unconverged.len()
    );
    // Measured (macOS): residual 3.4e-4, δ 1.0e-7, under the #791
    // threshold. δ is second order in the residual, so it stays small.
    assert!(delta < 1e-4, "tail δ = {delta:.3e}");

    // The checked solve: only converged pairs, the tail value is gone.
    let raw = dielectric_raw_candidates_p2(&mesh, &eps_r, &dof_mask, k0, 16, ceiling).unwrap();
    eprintln!(
        "SMF-28 checked: {} converged candidates, {} withheld ({} in window), {} steps",
        raw.cands.len(),
        raw.withheld,
        raw.withheld_in_window,
        raw.lanczos_steps
    );
    assert!(!raw.cands.is_empty());
    for c in &raw.cands {
        assert!(
            c.residual <= DIELECTRIC_RESIDUAL_TOL,
            "residual {:.3e}",
            c.residual
        );
        assert!(
            rayleigh_identity_violation(c, k0) < 1e-10,
            "converged candidate δ = {:.3e}",
            rayleigh_identity_violation(c, k0)
        );
        for &(lambda, _, _) in &unconverged {
            assert!(
                (c.beta_sq - lambda).abs() > 1e-9 * lambda,
                "an unconverged tail value must not be returned"
            );
        }
    }
    assert!(rayleigh_consistent(
        raw.cands
            .iter()
            .find(|c| c.beta_sq < ceiling.unwrap_or(n_core).powi(2) * k0_sq)
            .expect("an in-window converged candidate"),
        k0,
        e_core,
        e_clad
    ));
}

/// **Requesting k modes returns k converged modes, or an explicit
/// shortfall** (issue #798), on SMF-28 and the SiN strip (p=2). Each
/// returned mode is a converged raw candidate. When fewer than `k`
/// converged guided candidates exist, the withheld in-window count is
/// non-zero, so the shortfall is visible.
#[test]
fn p2_requesting_k_modes_returns_converged_modes() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let k = 4;
    let smf = {
        let (mesh, _tags, eps_r) = fiber_fixture(1.4504, 1.4447, 4.1);
        let mask = disk_pec_interior_dofs2(&mesh, 6.0 * 4.1);
        (mesh, eps_r, mask)
    };
    let sin = {
        let (mesh, eps_r, _m1) =
            strip_fixture((30, 30), (3.0, 3.0), (1.2, 0.6), (4.0, 1.45_f64 * 1.45));
        let mask = rect_pec_interior_dofs2(&mesh, 3.0, 3.0);
        (mesh, eps_r, mask)
    };
    for (label, (mesh, eps_r, mask)) in [("SMF-28", smf), ("SiN strip", sin)] {
        let modes = solve_dielectric_modes2(&mesh, &eps_r, &mask, k0, k).unwrap();
        let ceiling = physical_index_ceiling(&mesh, &eps_r, k0);
        let raw = dielectric_raw_candidates_p2(&mesh, &eps_r, &mask, k0, (k + 8).max(16), ceiling)
            .unwrap();
        eprintln!(
            "{label}: {} of {k} modes, {} converged candidates, {} withheld \
             ({} in window), {} Lanczos steps; n_eff = {:?}",
            modes.len(),
            raw.cands.len(),
            raw.withheld,
            raw.withheld_in_window,
            raw.lanczos_steps,
            modes.iter().map(|m| m.n_eff).collect::<Vec<_>>()
        );
        assert!(
            modes.len() == k || raw.withheld_in_window > 0,
            "{label}: {} < {k} modes with no reported shortfall",
            modes.len()
        );
        for m in &modes {
            let c = raw
                .cands
                .iter()
                .find(|c| (c.beta_sq - m.beta_sq).abs() <= 1e-12 * m.beta_sq)
                .expect("every returned mode is a raw candidate");
            assert!(
                c.residual <= DIELECTRIC_RESIDUAL_TOL,
                "{label}: returned mode n_eff = {:.6} has residual {:.3e}",
                m.n_eff,
                c.residual
            );
        }
    }
}

/// **p=1 curl floor scales with the index contrast** (issue #794): the
/// same formula as p=2, so the weakly-guiding SMF-28 floor sits far below
/// the exact in-window bound and the high-contrast Si/SiO₂ floor stays at
/// exactly `3e-2`.
#[test]
fn p1_curl_floor_scales_with_index_contrast() {
    let (e_core, e_clad) = (1.4504_f64.powi(2), 1.4447_f64.powi(2));
    let floor = physical_curl_floor(e_core, e_clad);
    assert!(
        (floor - GUIDED_CURL_FLOOR_FRACTION * (e_core - e_clad) / e_clad).abs() < 1e-18,
        "SMF-28 p=1 floor {floor:.3e} must be the scaled contrast"
    );
    assert!(floor < 0.04 * (e_core - e_clad) / e_core);
    for (n_core, n_clad) in [(3.45_f64, 1.45_f64), (3.48, 1.444)] {
        assert_eq!(
            physical_curl_floor(n_core * n_core, n_clad * n_clad),
            HIGH_CONTRAST_CURL_FLOOR,
            "Si/SiO2 ({n_core}, {n_clad}) p=1 floor must be unchanged"
        );
    }
    assert_eq!(physical_curl_floor(2.0, 2.0), physical_curl_floor_pml());
    // p=1 and p=2 share the formula.
    for i in 1..=50 {
        let e = 2.0 + 0.2 * f64::from(i);
        assert_eq!(physical_curl_floor(e, 2.0), physical_curl_floor_p2(e, 2.0));
    }
}

/// **Weak-contrast p=1 solve finds the fundamental** (issue #794). On the
/// SMF-28 PEC disk the guided curl ratios (`r < 7.8e-3`) all sit below the
/// old fixed `3e-2` p=1 floor, so `solve_dielectric_modes` used to return
/// no guided mode. With the contrast-scaled floor it returns a
/// core-guided, in-window, converged fundamental. The core fraction comes
/// from the exact identity `⟨ε⟩_x = ε_clad + f·Δε` for this two-material
/// cross-section.
#[test]
fn p1_dielectric_solve_weak_contrast_smf28() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n_core, n_clad) = (1.4504_f64, 1.4447_f64);
    let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
    let (mesh, _tags, eps_r) = fiber_fixture(n_core, n_clad, 4.1);
    let (_edges, interior) = disk_pec_interior_edges(&mesh, 6.0 * 4.1);

    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 1).expect("p=1 solve");
    let fund = modes
        .first()
        .expect("p=1 SMF-28 must return its fundamental");
    assert!(fund.n_eff > n_clad && fund.n_eff < n_core);

    let ops = assemble_2d_nedelec_sparse_interior(&mesh, &eps_r, &interior).unwrap();
    let x: Vec<f64> = fund
        .e_edges
        .iter()
        .zip(&interior)
        .filter(|(_, keep)| **keep)
        .map(|(v, _)| *v)
        .collect();
    let xkx = sparse_quadratic_form(ops.k.as_ref(), &x);
    let xmx = sparse_quadratic_form(ops.m_eps.as_ref(), &x);
    let xm1x = sparse_quadratic_form(ops.m1.as_ref(), &x);
    let r = xkx / (k0 * k0 * xmx);
    let eps_x = xmx / xm1x;
    let core_fraction = (eps_x - e_clad) / (e_core - e_clad);
    let oracle = crate::analytic::fiber::fiber_lp_neff(n_core, n_clad, 4.1, k0, 0, 1)
        .expect("LP01 is always guided");
    eprintln!(
        "p=1 SMF-28: n_eff = {:.6} (LP01 {oracle:.6}), r = {r:.3e}, core fraction = \
         {core_fraction:.3}, floor = {:.3e}",
        fund.n_eff,
        physical_curl_floor(e_core, e_clad)
    );
    assert!(
        core_fraction > 0.6,
        "fundamental must be core-guided (core fraction {core_fraction:.3})"
    );
    // The old fixed floor would have rejected it.
    assert!(r < HIGH_CONTRAST_CURL_FLOOR && r > physical_curl_floor(e_core, e_clad));
    // A converged eigenpair: the Rayleigh identity holds to round-off.
    let delta = (r - (1.0 - fund.n_eff * fund.n_eff / eps_x)).abs();
    assert!(delta < 1e-10, "Rayleigh identity δ = {delta:.3e}");
}

/// **High-contrast p=1 solve is unchanged** (issues #794, #798): on the
/// Si/SiO₂ slab the p=1 floor is still `3e-2`, and the fundamental is the
/// historical value (`n_eff = 3.016965` on the (4,60) slab, the converged
/// first-pass pair of the pre-#798 solve).
#[test]
fn p1_dielectric_solve_high_contrast_unchanged() {
    let (n_core, n_clad) = (3.45_f64, 1.45_f64);
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (mesh, eps_r, interior) =
        slab_fixture(4, 60, 0.2, 3.0, 0.3, n_core * n_core, n_clad * n_clad);
    assert_eq!(
        physical_curl_floor(n_core * n_core, n_clad * n_clad),
        HIGH_CONTRAST_CURL_FLOOR
    );
    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).unwrap();
    eprintln!(
        "p=1 Si slab (4,60): n_eff = {:?}",
        modes.iter().map(|m| m.n_eff).collect::<Vec<_>>()
    );
    assert!((modes[0].n_eff - 3.016_965).abs() < 1e-6);
}

/// **M-orthonormality + sign pin** of the returned dielectric mode:
/// the transverse profile is M₁-orthonormal (`eᵀ M₁ e = 1`) and its
/// largest-magnitude component is non-negative.
#[test]
fn dielectric_mode_profile_normalized_and_sign_pinned() {
    let n_core = 3.45_f64;
    let n_clad = 1.45_f64;
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let d = 0.30_f64;
    let (w, h) = (0.20_f64, 3.0_f64);
    let (mesh, eps_r, interior) = slab_fixture(4, 60, w, h, d, n_core * n_core, n_clad * n_clad);
    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 1).expect("dielectric solve");
    assert!(!modes.is_empty());
    let m0 = &modes[0];

    // eᵀ M₁ e = 1 in the full-edge representation (M₁ = unweighted).
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let (_k, m1) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);
    let n_edges = m1.nrows();
    let mut quad = 0.0_f64;
    for p in 0..n_edges {
        for q in 0..n_edges {
            quad += m0.e_edges[p] * m1[(p, q)] * m0.e_edges[q];
        }
    }
    assert!(
        (quad - 1.0).abs() < 1e-9,
        "eᵀ M₁ e = {quad} ≠ 1 (not M-orthonormal)"
    );

    // Sign pin: largest-magnitude component non-negative.
    let val = m0
        .e_edges
        .iter()
        .copied()
        .max_by(|a, b| {
            a.abs()
                .partial_cmp(&b.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    assert!(
        val > 0.0,
        "sign-pin: largest-magnitude component must be > 0"
    );

    // β = n_eff · k0 consistency.
    assert!(
        (m0.beta - m0.n_eff * k0).abs() < 1e-9 * (m0.beta.abs().max(1.0)),
        "β {} ≠ n_eff·k0 {}",
        m0.beta,
        m0.n_eff * k0
    );
}

/// **Multi-resolution robustness of the spurious-mode filter** (Epic
/// #303 Phase 1B, issue #305 — the load-bearing classifier fix).
///
/// The earlier acceptance test pinned a single 4×80 mesh. The Judge's
/// refinement sweep showed the *old* `r > 1e-3` curl-energy floor let a
/// weakly-resolved gradient mode (`n_eff ≈ 3.32`, near `n_core`) pass
/// at `ny=60` and be promoted to a spurious fundamental (17.75 % error
/// vs the slab oracle). This test runs `solve_dielectric_modes` at
/// **several** resolutions (ny ∈ {40, 60, 80, 120}, all near-isotropic
/// cells) and asserts that at every one the classifier returns the
/// *genuine* guided fundamental rather than a near-ceiling spurious
/// mode.
///
/// Two assertions, separating *filter robustness* from *mesh
/// accuracy*:
/// 1. **No spurious promotion** — `n_eff < 3.0` at every resolution.
///    With the old `1e-3` floor, ny=60 returned `n_eff ≈ 3.32`; with
///    the recalibrated floor it returns the genuine `n_eff ≈ 2.76`.
///    This is the load-bearing robustness claim.
/// 2. **Convergent accuracy** — `rel_err ≤ 2.5 %`. The returned mode is
///    the true fundamental, but its accuracy is limited by the mesh:
///    the genuine discretization error is ~1.3 % at ny=40, ~2.2 % at
///    the coarse ny=60/nx=3 grid, and tightens to ~0.7 % at ny=80/120.
///    The ≤ 1 % *converged* accuracy is pinned separately by
///    `slab_fundamental_neff_matches_oracle` at 4×80; here we only
///    require that the selected mode genuinely converges toward the
///    oracle (≤ 2.5 %), never the 17.75 % spurious outlier.
#[test]
fn dielectric_fundamental_robust_across_resolution() {
    let n_core = 3.45_f64;
    let n_clad = 1.45_f64;
    let eps_core = n_core * n_core;
    let eps_clad = n_clad * n_clad;
    let lambda = 1.55_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;
    let d = 0.22_f64;
    let w = 0.20_f64;
    let h = 4.0_f64;
    let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);

    // (nx, ny) kept ~isotropic: dx = w/nx ≈ dy = h/ny.
    for (nx, ny) in [(2usize, 40usize), (3, 60), (4, 80), (6, 120)] {
        let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);
        let modes =
            solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
        assert!(
            !modes.is_empty(),
            "ny={ny}: expected at least the fundamental mode"
        );
        let n_eff_fem = modes[0].n_eff;
        let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
        eprintln!(
            "robust sweep ny={ny} nx={nx}: n_eff_fem={n_eff_fem:.6}, \
             oracle={n_eff_oracle:.6}, rel={:.3}%",
            100.0 * rel_err
        );
        // The selected fundamental must be the genuine guided mode, not
        // a near-ceiling spurious one. The old filter returned
        // n_eff≈3.32 at ny=60 (17.75%); guard explicitly against it.
        assert!(
            n_eff_fem < 3.0,
            "ny={ny}: returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
        );
        assert!(
            rel_err < 0.025,
            "ny={ny}: fundamental n_eff {n_eff_fem:.6} vs oracle \
             {n_eff_oracle:.6} ({:.3}% > 2.5%)",
            100.0 * rel_err
        );
    }
}

/// **Off-grid regression: production solver returns the genuine
/// fundamental at an un-pinned mesh** (Epic #303 Phase 1B, issue #305 —
/// Judge feedback on PR #308).
///
/// The multi-resolution sweep above pins ny ∈ {40,60,80,120}. None of
/// those happens to surface a dominant *out-of-window* high-curl spike,
/// so a data-driven gap-widening floor stayed inert there. At the
/// off-grid refinement point **ny=100/nx=5** the raw spectrum contains
/// an out-of-window eigenpair at `n_eff ≈ 3.52`, `r ≈ 35` (above the
/// `n_core` ceiling), which a widening rule would fold into the floor,
/// driving it to `≈ 9.4` — above the genuine band ceiling `r ≈ 0.19` —
/// so `solve_dielectric_modes` returned **zero** modes. This test pins
/// the *production* path (`solve_dielectric_modes`, not the raw-candidate
/// harness) at that exact mesh and asserts a non-empty bound set whose
/// fundamental is the genuine guided mode (`n_clad < n_eff < n_core`,
/// `n_eff < 3.0`, and within 2.5 % of the slab oracle — the same coarse-
/// mesh tolerance as the multi-resolution sweep).
#[test]
fn dielectric_fundamental_off_grid_ny100() {
    let n_core = 3.45_f64;
    let n_clad = 1.45_f64;
    let eps_core = n_core * n_core;
    let eps_clad = n_clad * n_clad;
    let lambda = 1.55_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;
    let d = 0.22_f64;
    let w = 0.20_f64;
    let h = 4.0_f64;
    let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);

    // Off-grid mesh the Judge used: ny=100/nx=5 (a natural refinement
    // point between the pinned ny=80 and ny=120). Previously the
    // gap-widening floor swallowed the genuine band here and the solver
    // returned zero modes.
    let (nx, ny) = (5usize, 100usize);
    let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);
    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
    assert!(
        !modes.is_empty(),
        "off-grid ny={ny}/nx={nx}: production solver returned ZERO bound \
         modes (the gap-widening regression); expected ≥1"
    );
    // Every returned mode must lie strictly inside the bound window.
    for m in &modes {
        assert!(m.guided, "returned mode must be guided");
        assert!(
            m.n_eff > n_clad && m.n_eff < n_core,
            "ny={ny}: n_eff {} outside (n_clad, n_core)",
            m.n_eff
        );
    }
    let n_eff_fem = modes[0].n_eff;
    let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
    eprintln!(
        "off-grid ny={ny} nx={nx}: n_eff_fem={n_eff_fem:.6}, \
         oracle={n_eff_oracle:.6}, rel={:.3}%",
        100.0 * rel_err
    );
    // Genuine guided fundamental, not a near-ceiling spurious mode.
    assert!(
        n_eff_fem < 3.0,
        "ny={ny}: returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
    );
    // Coarse-mesh accuracy: same 2.5 % tolerance as the resolution sweep.
    assert!(
        rel_err < 0.025,
        "ny={ny}: fundamental n_eff {n_eff_fem:.6} vs oracle \
         {n_eff_oracle:.6} ({:.3}% > 2.5%)",
        100.0 * rel_err
    );
}

/// **Uniform-ε reduction to the metallic dispersion** — the *solver*,
/// not a hand-computed scalar, is what is pinned (issue #305, Judge
/// feedback on PR #308).
///
/// With a uniform `ε_r ≡ ε` on a PEC rectangle, `M_ε = ε M₁`, so the
/// dielectric pencil `A x = β² M₁ x` with `A = k₀² M_ε − K` reduces to
/// `(k₀² ε M₁ − K) x = β² M₁ x`, i.e. `K x = (ε k₀² − β²) M₁ x`. Hence
/// every metallic cutoff eigenpair `K x = k_c² M₁ x` reappears in the
/// dielectric pencil with `β² = ε k₀² − k_c²`. We verify this
/// **end-to-end**: we recover the dielectric pencil's eigenpairs via
/// [`dielectric_raw_candidates_with_target`] (the same solver core
/// `solve_dielectric_modes` uses — the bound-window classifier is
/// bypassed only because a homogeneous medium has the empty window
/// `(√ε, √ε)`), take the dominant curl-carrying mode, and check its
/// `β²` equals `ε k₀² − k_c²` for the dominant metallic cutoff `k_c`
/// from the already-validated [`solve_rect_waveguide_modes`] on the
/// *same* mesh.
#[test]
fn uniform_epsilon_reduces_to_metallic_dispersion() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    let eps = 4.0_f64; // uniform

    // Dominant metallic cutoff from the already-validated solver.
    let metallic = solve_rect_waveguide_modes(&mesh, a, b, 1).expect("metallic solve");
    let kc = metallic[0].k_c;

    // Choose k0 so the dominant mode is above cutoff:
    // β² = ε k₀² − k_c² > 0 ⇒ k₀ > k_c/√ε.
    let k0 = 2.0 * kc / eps.sqrt();
    let beta_sq_expected = eps * k0 * k0 - kc * kc;

    // Run the dielectric *solver core* on the SAME PEC mask and ε ≡ ε.
    // The shift σ sits just below the ceiling ε k₀², so the dominant
    // (smallest-k_c) mode — the one with the largest β² below the
    // ceiling that carries curl energy — is recovered. (The pure
    // gradient nullspace sits exactly at β² = ε k₀² = the ceiling, with
    // r ≈ 0, and is excluded by the curl-energy floor.)
    let (_edges, interior) = rect_pec_interior_edges(&mesh, a, b);
    let eps_r = vec![eps; mesh.n_tris()];
    let cands = dielectric_raw_candidates_with_target(&mesh, &eps_r, &interior, k0, 16, None)
        .expect("dielectric core")
        .cands;
    assert!(!cands.is_empty(), "solver returned no eigenpairs");

    // Dominant curl-carrying mode = largest β² with non-negligible curl
    // energy (rejecting the gradient cluster at the ceiling).
    let floor = physical_curl_floor(eps, eps);
    let dominant = cands
        .iter()
        .filter(|c| c.curl_ratio > floor)
        .max_by(|x, y| {
            x.beta_sq
                .partial_cmp(&y.beta_sq)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("a curl-carrying mode below the ceiling");
    let beta_sq_fem = dominant.beta_sq;
    let n_eff_fem = beta_sq_fem.max(0.0).sqrt() / k0;
    let n_eff_expected = beta_sq_expected.sqrt() / k0;
    eprintln!(
        "uniform-ε reduction: kc={kc:.6}, k0={k0:.6}, ε={eps}; \
         β²_fem={beta_sq_fem:.6} vs β²_expected={beta_sq_expected:.6}; \
         n_eff_fem={n_eff_fem:.6} vs {n_eff_expected:.6}"
    );

    // The solver's eigenvalue must reproduce the metallic dispersion
    // β² = ε k₀² − k_c² to discretization-consistency tolerance (the
    // two solvers use different shifts σ and the same K, M₁, so the
    // eigenvalue agreement is at solver tolerance, not bit-exact).
    let rel = (beta_sq_fem - beta_sq_expected).abs() / beta_sq_expected.abs().max(1.0);
    assert!(
        rel < 1e-6,
        "dielectric β² {beta_sq_fem} ≠ metallic ε k₀² − k_c² \
         {beta_sq_expected} (rel {rel:.3e})"
    );
    // And therefore n_eff² = ε − (k_c/k₀)².
    let rhs = eps - (kc / k0) * (kc / k0);
    assert!(
        (n_eff_fem * n_eff_fem - rhs).abs() < 1e-5 * rhs.abs().max(1.0),
        "n_eff² {} ≠ ε − (kc/k0)² {rhs}",
        n_eff_fem * n_eff_fem
    );
}

/// **Port-mode solve returns converged modes only** (issue #798,
/// cross-check from PR #809). On the `b/16` rectangular guide the
/// historical `n_modes = 12` solve returned an unconverged tail: TE₃₁ off
/// by `1.06e-8` relative, and the near-degenerate pairs at `k_c² ≈ 39.31`
/// and `49.34` merged into single values, while `n_modes ≥ 14` was right
/// to `1e-12`. The residual-checked solve must give the same 12 lowest
/// modes whether asked for 12 or 20.
#[test]
fn port_mode_solve_tail_matches_larger_request() {
    let mesh = rect_tri_mesh(32, 16, 2.0, 1.0);
    let (edges, mask) = rect_pec_interior_edges(&mesh, 2.0, 1.0);
    let m12 = solve_waveguide_modes(&mesh, &edges, &mask, 12).expect("12-mode solve");
    let m20 = solve_waveguide_modes(&mesh, &edges, &mask, 20).expect("20-mode solve");
    assert_eq!(m12.len(), 12);
    for (i, (a, b)) in m12.iter().zip(&m20).enumerate() {
        let rel = (a.lambda - b.lambda).abs() / b.lambda;
        assert!(
            rel < 1e-11,
            "mode {i}: k_c² = {:.10} (n=12) vs {:.10} (n=20), rel {rel:.2e}",
            a.lambda,
            b.lambda
        );
    }
    // TE₃₁ and both members of the near-degenerate pairs.
    assert!((m12[6].lambda - 32.106_193_548_1).abs() < 1e-8);
    assert!((m12[8].lambda - m12[7].lambda) > 5e-4);
    assert!((m12[11].lambda - m12[10].lambda) > 1e-3);
}

/// Build a high-contrast **2-D strip** fixture: a rectangle
/// `[0,W]×[0,H]` with a finite-extent high-index **core rectangle** of
/// full lateral width `w_core` and full vertical thickness `d_core`,
/// centred at `(W/2, H/2)` and clad on *all four sides*. Triangles are
/// tagged by centroid: tag 1 (core) when the centroid is inside the
/// core rectangle, else tag 0 (clad). This is the SOI-strip analogue of
/// `slab_fixture` (which is invariant in x).
fn strip_fixture(
    (nx, ny): (usize, usize),
    (w, h): (f64, f64),
    (w_core, d_core): (f64, f64),
    (eps_core, eps_clad): (f64, f64),
) -> (TriMesh, Vec<f64>, Vec<bool>) {
    let mesh = rect_tri_mesh(nx, ny, w, h);
    let region_tags: Vec<i32> = mesh
        .tris
        .iter()
        .map(|t| {
            let xc = (mesh.nodes[t[0] as usize][0]
                + mesh.nodes[t[1] as usize][0]
                + mesh.nodes[t[2] as usize][0])
                / 3.0;
            let yc = (mesh.nodes[t[0] as usize][1]
                + mesh.nodes[t[1] as usize][1]
                + mesh.nodes[t[2] as usize][1])
                / 3.0;
            if (xc - 0.5 * w).abs() < 0.5 * w_core && (yc - 0.5 * h).abs() < 0.5 * d_core {
                1
            } else {
                0
            }
        })
        .collect();
    let eps_r =
        epsilon_r_from_region_tags(
            &region_tags,
            |tag| {
                if tag == 1 { eps_core } else { eps_clad }
            },
        );
    let (_edges, interior) = rect_pec_interior_edges(&mesh, w, h);
    (mesh, eps_r, interior)
}

/// **Physical-index-ceiling derivation** (Epic #303, issue #309).
///
/// The ceiling is derived from the geometry/materials, NOT fitted to a
/// target n_eff. This pins the two regimes:
///
/// 1. **2-D strip** (core confined in BOTH directions): the ceiling is
///    the `min` of the per-axis 1-D-slab limits — for the SOI strip the
///    vertical 220-nm slab (≈2.85) is the binding one (below the lateral
///    450-nm slab ≈3.24), and the ceiling is strictly below `n_core`.
/// 2. **Slab** (core spans the full width — x-invariant): no axis is
///    laterally confined, so `physical_index_ceiling` returns `None` and
///    the classifier keeps the `n_core` ceiling, preserving the
///    validated 1-D behaviour.
#[test]
fn physical_index_ceiling_derives_from_geometry() {
    let n_core = 3.48_f64;
    let n_clad = 1.444_f64;
    let eps_core = n_core * n_core;
    let eps_clad = n_clad * n_clad;
    let lambda = 1.55_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;

    // --- 2-D strip: ceiling = min(vertical, lateral) slab limit. ---
    let (w, h) = (2.0_f64, 2.0_f64);
    let (w_core, d_core) = (0.45_f64, 0.22_f64);
    let (mesh, eps_r, _interior) =
        strip_fixture((40, 40), (w, h), (w_core, d_core), (eps_core, eps_clad));
    let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
        .expect("a 2-D-confined strip must yield a finite ceiling");

    // Independent per-axis 1-D slab limits (the EIM building blocks).
    let vslab = slab_te0_neff(n_core, n_clad, d_core, k0); // ≈ 2.85
    let lslab = slab_te0_neff(n_core, n_clad, w_core, k0); // ≈ 3.24
    let expected = vslab.min(lslab);
    eprintln!(
        "ceiling derivation: vertical-slab(d={d_core})={vslab:.4}, \
         lateral-slab(w={w_core})={lslab:.4}, ceiling={ceiling:.4}, n_core={n_core}"
    );
    // The ceiling is the smaller (binding) slab limit, derived from the
    // core extents the fixture tagged — and strictly below n_core. The
    // core extent recovered from the mesh is within one cell of the
    // requested core size, so allow a small mesh-discretization slack.
    assert!(
        ceiling < n_core,
        "ceiling {ceiling} must be strictly below n_core {n_core}"
    );
    // The binding limit is the vertical (220-nm) slab, not the lateral
    // (450-nm) one; the ceiling is within ~one cell of resolving the
    // requested 220-nm thickness (centroid tagging recovers the core
    // extent to within a cell ⇒ slab limit slack ≲ 0.1).
    assert!(
        (ceiling - expected).abs() < 0.1,
        "ceiling {ceiling} should match min-slab (vertical) limit {expected}"
    );
    assert!(
        ceiling < lslab,
        "ceiling {ceiling} must be below the lateral slab limit {lslab} (vertical binds)"
    );

    // --- Slab (x-invariant): no lateral confinement ⇒ None. ---
    let (sw, sh, sd) = (0.20_f64, 4.0_f64, 0.22_f64);
    let (smesh, seps_r, _si) = slab_fixture(4, 80, sw, sh, sd, eps_core, eps_clad);
    assert!(
        physical_index_ceiling(&smesh, &seps_r, k0).is_none(),
        "an x-invariant slab must yield no 2-D ceiling (keep n_core)"
    );

    // --- Uniform ε: no contrast ⇒ None. ---
    let umesh = rect_tri_mesh(8, 8, 1.0, 1.0);
    let ueps = vec![eps_core; umesh.n_tris()];
    assert!(
        physical_index_ceiling(&umesh, &ueps, k0).is_none(),
        "uniform ε has no guiding structure ⇒ no ceiling"
    );
}

/// **High-contrast 2-D SOI strip: genuine fundamental is returned
/// FIRST** (Epic #303, issue #309 — the load-bearing hardening).
///
/// Geometry: a silicon strip (n_Si = 3.48) of 220 nm × 450 nm buried in
/// SiO₂ (n = 1.444) at λ = 1550 nm — confined in *both* transverse
/// directions. Before this fix the solver returned a dense ladder of
/// unphysical near-`n_core` modes (n_eff ≈ 3.0–3.37) that passed the
/// slab-calibrated curl floor and outranked the genuine fundamental
/// (n_eff ≈ 2.6, matching an effective-index-method / min-slab estimate
/// to a few %). The physical-index-ceiling rejection (derived from the
/// core geometry, NOT fitted) removes them, and the guided-band shift
/// makes the fundamental converge among the first few modes — so we can
/// request just a handful of modes (CI-fast).
///
/// Assertions:
/// - the returned fundamental n_eff is in-window `(n_SiO2, n_Si)`,
/// - it is below the derived physical 1-D-slab ceiling,
/// - it agrees with an independent EIM/min-slab estimate within a stated
///   tolerance,
/// - and NO returned mode exceeds the physical ceiling.
#[test]
fn high_contrast_soi_strip_fundamental_first() {
    let n_si = 3.48_f64;
    let n_sio2 = 1.444_f64;
    let eps_si = n_si * n_si; // ≈ 12.11
    let eps_sio2 = n_sio2 * n_sio2; // ≈ 2.085
    let lambda = 1.55_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;

    // SOI strip: 450 nm wide × 220 nm tall Si core, buried in SiO₂.
    // Use a compact window (≈ a quarter-µm of cladding each side — the
    // ε≈12.1/2.085 contrast confines the mode tightly so the PEC walls
    // are immaterial) with the core resolved by several cells in each
    // direction. The mesh is kept small enough for a CI-fast solve: the
    // dielectric pencil is assembled densely, so cost scales steeply
    // with the edge count. Cells are roughly isotropic to keep the
    // spectrum clean.
    let w_core = 0.45_f64;
    let d_core = 0.22_f64;
    let (w, h) = (1.0_f64, 1.0_f64);
    let (nx, ny) = (32usize, 32usize);
    let (mesh, eps_r, interior) =
        strip_fixture((nx, ny), (w, h), (w_core, d_core), (eps_si, eps_sio2));

    // Physical ceiling derived purely from geometry/materials.
    let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
        .expect("2-D strip must yield a finite physical ceiling");

    // Independent EIM / min-slab estimate (NOT used to select the mode —
    // only to validate the answer). The genuine 2-D n_eff sits below
    // both 1-D slab limits; a standard effective-index-method estimate
    // for this SOI strip is ≈2.6.
    let eim_estimate = 2.6_f64;

    // CI-fast: request only a few modes — the guided-band shift puts the
    // fundamental among the first recovered eigenpairs.
    let modes = solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 4).expect("dielectric solve");
    assert!(
        !modes.is_empty(),
        "expected at least the genuine fundamental of the SOI strip"
    );

    let n_eff_fem = modes[0].n_eff;
    eprintln!(
        "SOI strip fundamental: n_eff_fem={n_eff_fem:.6}, ceiling={ceiling:.6}, \
         EIM≈{eim_estimate}, window=({n_sio2}, {n_si})"
    );

    // In-window.
    assert!(
        n_eff_fem > n_sio2 && n_eff_fem < n_si,
        "fundamental n_eff {n_eff_fem} outside (n_SiO2, n_Si)"
    );
    // Below the derived physical ceiling — and NO returned mode exceeds
    // it (the load-bearing claim: spurious near-n_core modes removed).
    for m in &modes {
        assert!(
            m.n_eff <= ceiling + 1e-9,
            "returned mode n_eff {} exceeds physical ceiling {ceiling}",
            m.n_eff
        );
    }
    // Genuine fundamental, not a near-ceiling spurious mode.
    assert!(
        n_eff_fem < 3.0,
        "returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
    );
    // Agreement with the independent EIM estimate.
    let rel = (n_eff_fem - eim_estimate).abs() / eim_estimate;
    eprintln!("SOI strip EIM agreement: {:.2}%", 100.0 * rel);
    assert!(
        rel < 0.06,
        "SOI fundamental n_eff {n_eff_fem:.4} vs EIM {eim_estimate} \
         ({:.2}% > 6%)",
        100.0 * rel
    );
}

// ---- Phase 2.5B: global p=2 DOF numbering + ε assembly + signs ----

/// DOF count is exactly `2·n_edges + 2·n_tris`, and the assembled
/// matrices are square at that size.
#[test]
fn p2_global_dof_count() {
    let mesh = rect_tri_mesh(3, 2, 1.0, 0.7);
    let n_edges = mesh.edges().len();
    let n_tris = mesh.n_tris();
    let expect = 2 * n_edges + 2 * n_tris;
    assert_eq!(n_dof_2d_nedelec2(&mesh), expect);

    let eps = vec![1.0; n_tris];
    let (k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
    assert_eq!(k.nrows(), expect);
    assert_eq!(k.ncols(), expect);
    assert_eq!(m.nrows(), expect);
    assert_eq!(m.ncols(), expect);
}

/// Global `K` and `M` are symmetric to tolerance (a non-uniform ε makes
/// the test bite the sign bookkeeping, not just a uniform scale).
#[test]
fn p2_global_matrices_symmetric() {
    let mesh = rect_tri_mesh(3, 3, 1.3, 0.9);
    let eps: Vec<f64> = (0..mesh.n_tris())
        .map(|t| 1.0 + 0.5 * (t % 4) as f64)
        .collect();
    let (k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
    let n = k.nrows();
    for i in 0..n {
        for j in 0..n {
            assert!(
                (k[(i, j)] - k[(j, i)]).abs() < 1e-10,
                "K not symmetric at ({i},{j}): {} vs {}",
                k[(i, j)],
                k[(j, i)]
            );
            assert!(
                (m[(i, j)] - m[(j, i)]).abs() < 1e-10,
                "M not symmetric at ({i},{j}): {} vs {}",
                m[(i, j)],
                m[(j, i)]
            );
        }
    }
}

/// **p=1-subset check (load-bearing):** restricting the assembled p=2
/// system to the Whitney (even-indexed edge) DOFs `{2·0, 2·1, …}`
/// reproduces `assemble_2d_nedelec_with_epsilon` to tight tolerance.
/// This validates both the DOF numbering and the per-DOF Whitney signs
/// at the *global* (shared-edge) level.
#[test]
fn p2_global_p1_subset_matches() {
    let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
    let eps: Vec<f64> = (0..mesh.n_tris())
        .map(|t| 1.0 + 0.25 * (t % 3) as f64)
        .collect();

    let (k1, m1) = assemble_2d_nedelec_with_epsilon(&mesh, &eps);
    let (k2, m2) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

    let n_edges = mesh.edges().len();
    for e_i in 0..n_edges {
        let gi = 2 * e_i; // Whitney DOF of edge e_i in the p=2 numbering
        for e_j in 0..n_edges {
            let gj = 2 * e_j;
            assert!(
                (k2[(gi, gj)] - k1[(e_i, e_j)]).abs() < 1e-10,
                "K p1-subset mismatch at edges ({e_i},{e_j}): {} vs {}",
                k2[(gi, gj)],
                k1[(e_i, e_j)]
            );
            assert!(
                (m2[(gi, gj)] - m1[(e_i, e_j)]).abs() < 1e-10,
                "M p1-subset mismatch at edges ({e_i},{e_j}): {} vs {}",
                m2[(gi, gj)],
                m1[(e_i, e_j)]
            );
        }
    }
}

/// Materialize a sparse `SparseColMat` into a dense `Mat<f64>` for an
/// entry-for-entry comparison against the dense assembler's output.
fn sparse_to_dense(a: SparseColMatRef<'_, usize, f64>) -> Mat<f64> {
    let mut out = Mat::<f64>::zeros(a.nrows(), a.ncols());
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    for j in 0..a.ncols() {
        for k in cp[j]..cp[j + 1] {
            out[(ri[k], j)] += v[k];
        }
    }
    out
}

/// Assert two dense matrices are equal entry-for-entry to `tol`.
fn assert_dense_eq(a: &Mat<f64>, b: &Mat<f64>, tol: f64, what: &str) {
    assert_eq!(a.nrows(), b.nrows(), "{what}: row count mismatch");
    assert_eq!(a.ncols(), b.ncols(), "{what}: col count mismatch");
    for i in 0..a.nrows() {
        for j in 0..a.ncols() {
            let d = (a[(i, j)] - b[(i, j)]).abs();
            assert!(
                d < tol,
                "{what}: entry ({i},{j}) mismatch {} vs {} (Δ={d:.3e})",
                a[(i, j)],
                b[(i, j)]
            );
        }
    }
}

/// **Issue #327 headline correctness gate:** the direct sparse
/// interior-restricted assembly (`assemble_2d_nedelec_sparse_interior`,
/// p=1) must equal the previous dense path
/// `apply_pec_2d(&assemble_2d_nedelec_with_epsilon(…))` **entry for
/// entry** for K, M_ε and M₁, on a small mesh with NON-uniform ε.
#[test]
fn sparse_interior_p1_matches_dense_nonuniform_eps() {
    let mesh = rect_tri_mesh(5, 4, 1.3, 0.9);
    // Non-uniform ε: three distinct values cycled across triangles.
    let eps: Vec<f64> = (0..mesh.n_tris())
        .map(|t| 1.0 + 0.37 * (t % 3) as f64 + 0.11 * (t % 5) as f64)
        .collect();
    let (_edges, interior) = rect_pec_interior_edges(&mesh, 1.3, 0.9);

    // Dense reference path.
    let (k_dense, m_eps_dense) = assemble_2d_nedelec_with_epsilon(&mesh, &eps);
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let (_k1, m1_dense_full) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);
    let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
    let (_k1_int, m1_int_dense) = apply_pec_2d(&k_dense, &m1_dense_full, &interior);

    // Sparse-direct path.
    let ops = assemble_2d_nedelec_sparse_interior(&mesh, &eps, &interior).unwrap();
    assert_eq!(ops.dim, k_int_dense.nrows(), "interior dim mismatch (p=1)");

    assert_dense_eq(
        &sparse_to_dense(ops.k.as_ref()),
        &k_int_dense,
        1e-12,
        "K p1",
    );
    assert_dense_eq(
        &sparse_to_dense(ops.m_eps.as_ref()),
        &m_eps_int_dense,
        1e-12,
        "M_eps p1",
    );
    assert_dense_eq(
        &sparse_to_dense(ops.m1.as_ref()),
        &m1_int_dense,
        1e-12,
        "M1 p1",
    );
}

/// **Issue #327 headline correctness gate (p=2):** the direct sparse
/// interior-restricted assembly (`assemble_2d_nedelec2_sparse_interior`)
/// must equal `apply_pec_2d(&assemble_2d_nedelec2_with_epsilon(…))` entry
/// for entry for K, M_ε and M₁, with NON-uniform ε and the p=2
/// interior-DOF mask. This exercises the per-DOF Whitney orientation
/// signs through the sparse scatter-add.
#[test]
fn sparse_interior_p2_matches_dense_nonuniform_eps() {
    let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
    let eps: Vec<f64> = (0..mesh.n_tris())
        .map(|t| 1.0 + 0.29 * (t % 4) as f64 + 0.13 * (t % 3) as f64)
        .collect();
    let interior = rect_pec_interior_dofs2(&mesh, 1.1, 0.8);

    let (k_dense, m_eps_dense) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let (_k1, m1_dense_full) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps_ones);
    let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
    let (_k1_int, m1_int_dense) = apply_pec_2d(&k_dense, &m1_dense_full, &interior);

    let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps, &interior).unwrap();
    assert_eq!(ops.dim, k_int_dense.nrows(), "interior dim mismatch (p=2)");

    assert_dense_eq(
        &sparse_to_dense(ops.k.as_ref()),
        &k_int_dense,
        1e-12,
        "K p2",
    );
    assert_dense_eq(
        &sparse_to_dense(ops.m_eps.as_ref()),
        &m_eps_int_dense,
        1e-12,
        "M_eps p2",
    );
    assert_dense_eq(
        &sparse_to_dense(ops.m1.as_ref()),
        &m1_int_dense,
        1e-12,
        "M1 p2",
    );
}

/// The sparse pencil `A = k₀² M_ε − K` must equal the dense
/// `k₀² M_ε,int − K_int` entry for entry (the operator the eigensolve
/// actually consumes), p=2, non-uniform ε.
#[test]
fn sparse_pencil_a_matches_dense_p2() {
    let mesh = rect_tri_mesh(4, 3, 1.1, 0.8);
    let eps: Vec<f64> = (0..mesh.n_tris())
        .map(|t| 1.0 + 0.4 * (t % 3) as f64)
        .collect();
    let interior = rect_pec_interior_dofs2(&mesh, 1.1, 0.8);
    let k0 = 2.7_f64;
    let k0_sq = k0 * k0;

    let (k_dense, m_eps_dense) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);
    let (k_int_dense, m_eps_int_dense) = apply_pec_2d(&k_dense, &m_eps_dense, &interior);
    let dim = k_int_dense.nrows();
    let mut a_dense = Mat::<f64>::zeros(dim, dim);
    for i in 0..dim {
        for j in 0..dim {
            a_dense[(i, j)] = k0_sq * m_eps_int_dense[(i, j)] - k_int_dense[(i, j)];
        }
    }

    let ops = assemble_2d_nedelec2_sparse_interior(&mesh, &eps, &interior).unwrap();
    let a_sparse = sparse_pencil_a(ops.k.as_ref(), ops.m_eps.as_ref(), k0_sq).unwrap();
    assert_dense_eq(&sparse_to_dense(a_sparse.as_ref()), &a_dense, 1e-12, "A p2");
}

/// **Shared-edge sign consistency (the key orientation guard):** two
/// triangles sharing one edge. The two triangles traverse the shared
/// edge with opposite *local* orientation, so each contributes the
/// Whitney function with a `tri_edges` sign of opposite parity. The
/// per-DOF sign rule must make those contributions **reinforce** (not
/// cancel) on the shared Whitney DOF's diagonal, while the even gradient
/// `Q` and the interior bubbles are unaffected.
///
/// We verify this structurally: assemble the two-triangle mesh, find the
/// shared edge, and check (a) its Whitney diagonal `M` entry equals the
/// sum of the two elements' local Whitney `M` diagonals (same magnitude,
/// reinforcing — no spurious cancellation), and (b) the same for the
/// gradient `Q` diagonal, which carries sign `+1` on both elements.
#[test]
fn p2_shared_edge_sign_consistency() {
    // Two CCW triangles sharing the diagonal edge (0)-(2) on the unit
    // square. T0 = [0,1,2] traverses the diagonal 0→2 (local edge
    // (0,2), global-aligned → sign +1). T1 = [2,3,0] (a CCW relabelling
    // of the upper triangle) traverses the diagonal 2→0 (local edge
    // (2,0), against the global a<b direction → sign -1). The two
    // triangles therefore touch the shared Whitney DOF with OPPOSITE
    // orientation parity — the real guard: the sign rule must make the
    // diagonal contributions reinforce (sign² = +1) rather than cancel.
    let mesh = TriMesh {
        nodes: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        tris: vec![[0, 1, 2], [2, 3, 0]],
    };
    let edges = mesh.edges();
    let tri_edges = mesh.tri_edges();
    let n_edges = edges.len();
    let eps = vec![1.0; mesh.n_tris()];
    let (_k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

    // Identify the global edge shared by both triangles: nodes {0,2}.
    let shared = edges
        .iter()
        .position(|e| *e == [0, 2] || *e == [2, 0])
        .expect("shared edge (0,2) must exist");

    // Confirm the two triangles really do touch it with opposite parity
    // (otherwise the cancellation guard is vacuous).
    let signs: Vec<i8> = tri_edges
        .iter()
        .map(|row| {
            row.iter()
                .find(|&&(g, _)| g as usize == shared)
                .map(|&(_, s)| s)
                .expect("triangle touches shared edge")
        })
        .collect();
    assert_eq!(
        signs[0] * signs[1],
        -1,
        "test fixture must give opposite orientation parity on the shared edge"
    );

    // Local-Whitney / Q diagonal contributions from each triangle for
    // the shared edge, accumulated by hand to predict the global entry.
    let mut expect_w_diag = 0.0;
    let mut expect_q_diag = 0.0;
    for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
        // local edge index (within this triangle) that maps to `shared`
        let lk = row
            .iter()
            .position(|&(g, _)| g as usize == shared)
            .expect("each triangle touches the shared edge");
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_kl, ml, _a) = tri_nedelec2_local(&coords);
        // Whitney sign squares to +1 on the diagonal, so the diagonal
        // contribution is always positive and the two triangles ADD.
        expect_w_diag += ml[2 * lk][2 * lk];
        expect_q_diag += ml[2 * lk + 1][2 * lk + 1];
    }

    let gw = 2 * shared; // Whitney DOF
    let gq = 2 * shared + 1; // gradient DOF
    assert!(
        (m[(gw, gw)] - expect_w_diag).abs() < 1e-12,
        "shared-edge Whitney M diagonal: assembled {} vs expected {} \
         (sign cancellation/doubling bug)",
        m[(gw, gw)],
        expect_w_diag
    );
    assert!(
        (m[(gq, gq)] - expect_q_diag).abs() < 1e-12,
        "shared-edge gradient M diagonal: assembled {} vs expected {}",
        m[(gq, gq)],
        expect_q_diag
    );

    // The shared Whitney DOF must actually receive contributions from
    // BOTH triangles (guards against the orientation logic silently
    // routing one triangle elsewhere) — i.e. its diagonal exceeds either
    // single-element contribution.
    assert!(
        m[(gw, gw)] > 0.0 && expect_w_diag > 0.0,
        "shared Whitney DOF received no mass contribution"
    );
    let _ = n_edges;
}

/// **Gradient-`Q` orientation guard (issue #325):** the gradient edge
/// functions `Q = ∇(λ_a λ_b)` are *even* (symmetric under `a ↔ b`), so
/// they must scatter with sign `+1` regardless of the global edge
/// orientation — they do NOT flip the way the Whitney functions do.
///
/// The diagonal `Q–Q` checks in `p2_shared_edge_sign_consistency` cannot
/// catch a wrong `Q` sign, because `sign² = +1` squares any sign bug away.
/// The discriminating signal is a `Q–Q` **off-diagonal** entry across a
/// shared edge, where the cross sign `sign_i · sign_j` does NOT square
/// out. This test hand-accumulates every global `M` `Q–Q` entry with the
/// gradient sign forced to `+1` (the orientation-independent rule) and
/// asserts the assembler agrees. If the edge `esign` were wrongly applied
/// to the `Q` DOFs, the two triangles meeting at the shared diagonal —
/// which touch it with OPPOSITE orientation parity — would corrupt the
/// shared `Q`'s off-diagonal couplings to the other edges' `Q` DOFs, and
/// the hand reference (which uses `+1`) would disagree.
///
/// Concretely guards against the mutation "apply `esign` to local DOFs
/// `1, 3, 5`", which the full suite otherwise passed silently.
#[test]
fn p2_shared_edge_q_offdiagonal_orientation_invariant() {
    // Same two-triangle fixture as p2_shared_edge_sign_consistency: the
    // shared diagonal edge is traversed with opposite local orientation
    // by the two triangles, so a wrong Q sign rule WILL surface on a
    // Q–Q off-diagonal (cross sign does not square to +1).
    let mesh = TriMesh {
        nodes: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        tris: vec![[0, 1, 2], [2, 3, 0]],
    };
    let edges = mesh.edges();
    let tri_edges = mesh.tri_edges();
    let n_edges = edges.len();
    let eps = vec![1.0; mesh.n_tris()];
    let (_k, m) = assemble_2d_nedelec2_with_epsilon(&mesh, &eps);

    // Hand-accumulate the global Q–Q block with the orientation-INDEPENDENT
    // rule (gradient sign ≡ +1), exactly as the correct assembler should.
    let mut q_ref = Mat::<f64>::zeros(n_edges, n_edges);
    for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_kl, ml, _a) = tri_nedelec2_local(&coords);
        // Local gradient DOFs are the odd local indices 1, 3, 5, mapping
        // to global edges row[0..3]. Q carries sign +1 (no esign).
        for (ka, &(ga, _sa)) in row.iter().enumerate() {
            for (kb, &(gb, _sb)) in row.iter().enumerate() {
                q_ref[(ga as usize, gb as usize)] += ml[2 * ka + 1][2 * kb + 1];
            }
        }
    }

    // The assembler's Q DOF for global edge e is global index 2e+1.
    // Every entry — diagonal AND off-diagonal — must match the +1 rule.
    // The off-diagonal entries are the load-bearing ones: they FAIL if
    // esign is wrongly applied to Q (cross sign ≠ +1 across the shared
    // edge), while the diagonal would pass either way.
    let mut checked_offdiag = 0usize;
    for e_i in 0..n_edges {
        for e_j in 0..n_edges {
            let gi = 2 * e_i + 1;
            let gj = 2 * e_j + 1;
            assert!(
                (m[(gi, gj)] - q_ref[(e_i, e_j)]).abs() < 1e-12,
                "Q–Q M entry (edges {e_i},{e_j}) is orientation-dependent: \
                 assembled {} vs +1-rule reference {} — gradient esign bug",
                m[(gi, gj)],
                q_ref[(e_i, e_j)]
            );
            if e_i != e_j && q_ref[(e_i, e_j)].abs() > 1e-12 {
                checked_offdiag += 1;
            }
        }
    }

    // The guard is only meaningful if at least one nonzero Q–Q
    // off-diagonal was actually exercised (otherwise the assertion above
    // is vacuous and a Q-flip mutation could still slip through).
    assert!(
        checked_offdiag > 0,
        "fixture produced no nonzero Q–Q off-diagonal entries; \
         the orientation guard would be vacuous"
    );

    // Sharpen the guard at the shared edge specifically: its Q DOF must
    // couple (off-diagonal) to at least one other edge's Q DOF, and that
    // coupling must match the +1-rule reference. This is the exact entry
    // the Q-flip mutation corrupts.
    let shared = edges
        .iter()
        .position(|e| *e == [0, 2] || *e == [2, 0])
        .expect("shared edge (0,2) must exist");
    let gq_shared = 2 * shared + 1;
    let mut shared_offdiag_nonzero = false;
    for e_j in 0..n_edges {
        if e_j == shared {
            continue;
        }
        let gj = 2 * e_j + 1;
        if q_ref[(shared, e_j)].abs() > 1e-12 {
            shared_offdiag_nonzero = true;
            assert!(
                (m[(gq_shared, gj)] - q_ref[(shared, e_j)]).abs() < 1e-12,
                "shared-edge Q off-diagonal to edge {e_j} is \
                 orientation-dependent (gradient esign bug)"
            );
        }
    }
    assert!(
        shared_offdiag_nonzero,
        "shared-edge Q DOF has no nonzero off-diagonal coupling; \
         cannot discriminate a Q-flip mutation"
    );
}

/// p=2 interior mask for `rect_tri_mesh`: length matches the DOF count,
/// every interior-bubble DOF is interior, wall-aligned edge DOFs are
/// boundary, and the edge-DOF interior flags agree with the first-order
/// `rect_pec_interior_edges` mask (both DOFs of an edge share its flag).
#[test]
fn p2_rect_interior_mask() {
    let (w, h) = (1.0, 0.6);
    let mesh = rect_tri_mesh(3, 2, w, h);
    let n_edges = mesh.edges().len();
    let n_tris = mesh.n_tris();
    let mask = rect_pec_interior_dofs2(&mesh, w, h);
    assert_eq!(mask.len(), 2 * n_edges + 2 * n_tris);

    let (_edges, edge_interior) = rect_pec_interior_edges(&mesh, w, h);
    for (e, &interior) in edge_interior.iter().enumerate() {
        assert_eq!(mask[2 * e], interior, "Whitney DOF of edge {e}");
        assert_eq!(mask[2 * e + 1], interior, "gradient DOF of edge {e}");
    }
    // All interior bubble DOFs are interior.
    for (d, &interior) in mask.iter().enumerate().skip(2 * n_edges) {
        assert!(interior, "interior bubble DOF {d} must be interior");
    }
    // At least one boundary edge exists (the rectangle has walls).
    assert!(
        edge_interior.iter().any(|&b| !b),
        "rectangle must have wall-aligned (boundary) edges"
    );
}

/// p=2 interior mask for `disk_tri_mesh`: same structural guarantees,
/// agreeing with `disk_pec_interior_edges`.
#[test]
fn p2_disk_interior_mask() {
    let outer = 1.0;
    let (mesh, _tags) = disk_tri_mesh(0.4, outer, 2, 12);
    let n_edges = mesh.edges().len();
    let n_tris = mesh.n_tris();
    let mask = disk_pec_interior_dofs2(&mesh, outer);
    assert_eq!(mask.len(), 2 * n_edges + 2 * n_tris);

    let (_edges, edge_interior) = disk_pec_interior_edges(&mesh, outer);
    for (e, &interior) in edge_interior.iter().enumerate() {
        assert_eq!(mask[2 * e], interior, "Whitney DOF of edge {e}");
        assert_eq!(mask[2 * e + 1], interior, "gradient DOF of edge {e}");
    }
    for (d, &interior) in mask.iter().enumerate().skip(2 * n_edges) {
        assert!(interior, "interior bubble DOF {d} must be interior");
    }
    assert!(
        edge_interior.iter().any(|&b| !b),
        "disk must have far-wall (boundary) edges"
    );
}

// -------------------------------------------------------------------
// Phase-2.5C (Epic #318): p=2 de-Rham nullspace + curl-free exactness
// -------------------------------------------------------------------

/// The local p=2 d⁰ projection is **exact**: the gradient of every
/// order-2 scalar Lagrange basis function lies in the p=2 Nédélec edge
/// space, so the L²-projection residual is ~machine zero. This is the
/// `d⁰` exactness check demanded by Epic #318 (the first-kind order-2
/// de-Rham sequence is exact by construction).
#[test]
fn p2_discrete_gradient_is_exact_in_edge_space() {
    // A single non-degenerate triangle (not the reference) to exercise
    // the affine Jacobian.
    let coords = [[0.3, 0.1], [1.7, 0.4], [0.6, 1.9]];
    let (_k, m_local, _area) = tri_nedelec2_local(&coords);

    let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
        - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
    let g = [
        [
            (coords[1][1] - coords[2][1]) / det,
            (coords[2][0] - coords[1][0]) / det,
        ],
        [
            (coords[2][1] - coords[0][1]) / det,
            (coords[0][0] - coords[2][0]) / det,
        ],
        [
            (coords[0][1] - coords[1][1]) / det,
            (coords[1][0] - coords[0][0]) / det,
        ],
    ];
    let area_abs = 0.5 * det.abs();

    let eval_vecs = |lam: [f64; 3]| -> [[f64; 2]; 8] {
        let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
        let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
            [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
        };
        let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
            [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
        };
        let w0 = whitney(0, 1, l0, l1);
        let w1 = whitney(0, 2, l0, l2);
        let w2 = whitney(1, 2, l1, l2);
        let q0 = qgrad(0, 1, l0, l1);
        let q1 = qgrad(0, 2, l0, l2);
        let q2 = qgrad(1, 2, l1, l2);
        let i0 = [l2 * w0[0], l2 * w0[1]];
        let i1 = [l0 * w2[0], l0 * w2[1]];
        [w0, q0, w1, q1, w2, q2, i0, i1]
    };

    // RHS b_{i,s} = ∫ N_i · ∇φ_s.
    let mut rhs = [[0.0_f64; 6]; 8];
    for qrow in TRI_QUAD_DEG4.iter() {
        let lam = [qrow[0], qrow[1], qrow[2]];
        let w = qrow[3] * area_abs;
        let vecs = eval_vecs(lam);
        let sgrads = tri_scalar2_grads(&g, lam);
        for i in 0..8 {
            for s in 0..6 {
                rhs[i][s] += w * (vecs[i][0] * sgrads[s][0] + vecs[i][1] * sgrads[s][1]);
            }
        }
    }
    let coeffs = solve_8x6(&m_local, &rhs);

    // Residual ‖∇φ_s − Σ_i c_i N_i‖²_L2 (computed from the mass form)
    // must be machine-zero for every scalar basis function.
    for s in 0..6 {
        // ∫ |∇φ_s|²  and  cross/self terms via quadrature.
        let mut res2 = 0.0_f64;
        for qrow in TRI_QUAD_DEG4.iter() {
            let lam = [qrow[0], qrow[1], qrow[2]];
            let w = qrow[3] * area_abs;
            let vecs = eval_vecs(lam);
            let sgrads = tri_scalar2_grads(&g, lam);
            let mut recon = [0.0_f64; 2];
            for i in 0..8 {
                recon[0] += coeffs[i][s] * vecs[i][0];
                recon[1] += coeffs[i][s] * vecs[i][1];
            }
            let dx = sgrads[s][0] - recon[0];
            let dy = sgrads[s][1] - recon[1];
            res2 += w * (dx * dx + dy * dy);
        }
        assert!(
            res2 < 1e-18,
            "∇φ_{s} not exactly in the p=2 edge space: L2 residual² = {res2:.3e}"
        );
    }
}

/// The generalized p=2 de-Rham nullspace dimension equals the number of
/// **interior scalar-p2 DOFs** = (interior nodes) + (interior edges),
/// pinned on a known small structured mesh. Compare the p=1 count,
/// which is interior nodes alone.
#[test]
fn p2_spurious_dim_counts_interior_scalar_dofs() {
    let (nx, ny) = (4usize, 3usize);
    let (w, h) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(nx, ny, w, h);

    let (_edges, edge_interior) = rect_pec_interior_edges(&mesh, w, h);
    let node_interior = rect_pec_interior_nodes(&mesh, w, h);
    let dof_mask = rect_pec_interior_dofs2(&mesh, w, h);

    let n_int_nodes = node_interior.iter().filter(|&&b| b).count();
    let n_int_edges = edge_interior.iter().filter(|&&b| b).count();

    let p1_dim = spurious_dim_2d(&mesh, &edge_interior, &node_interior);
    let p2_dim = spurious_dim_2d_p2(&mesh, &dof_mask, &node_interior, &edge_interior);

    // p=1: interior nodes only.
    assert_eq!(p1_dim, n_int_nodes, "p=1 nullspace = interior nodes");
    // p=2: interior nodes + interior edges (the new Q edge DOFs).
    assert_eq!(
        p2_dim,
        n_int_nodes + n_int_edges,
        "p=2 nullspace must equal interior nodes ({n_int_nodes}) + interior edges \
         ({n_int_edges}); got {p2_dim}"
    );
    // The p=2 nullspace is strictly larger than p=1 (interior edges > 0).
    assert!(
        p2_dim > p1_dim,
        "p=2 nullspace ({p2_dim}) must exceed p=1 ({p1_dim})"
    );
}

// ----- Epic #303 PML-A (issue #331): UPML stretch tensor + PML mesh + complex p=2 assembly -----

/// Read a `c64` entry `(r, c)` from a `SparseColMat<usize, c64>`.
fn c64_entry(a: &SparseColMat<usize, c64>, r: usize, c: usize) -> c64 {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut acc = c64::new(0.0, 0.0);
    for k in cp[c]..cp[c + 1] {
        if ri[k] == r {
            acc += v[k];
        }
    }
    acc
}

/// Read an `f64` entry `(r, c)` from a `SparseColMat<usize, f64>`.
fn f64_entry(a: &SparseColMat<usize, f64>, r: usize, c: usize) -> f64 {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut acc = 0.0_f64;
    for k in cp[c]..cp[c + 1] {
        if ri[k] == r {
            acc += v[k];
        }
    }
    acc
}

/// PML-tagged disk mesh: valid CCW mesh, three-region tags correct by
/// centroid radius, PML annulus is the outer band, sane area fractions.
#[test]
fn disk_tri_mesh_pml_three_region_tags() {
    let core_r = 1.0;
    let clad_r = 2.0; // = r_pml_inner
    let outer_r = 3.0;
    let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 4, 24);
    assert_eq!(tags.len(), mesh.n_tris());

    // Every triangle CCW (positive signed area).
    for tri in &mesh.tris {
        let c = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_, _, sa) = tri_nedelec2_local(&c);
        assert!(sa > 0.0, "triangle must be CCW; got area {sa}");
    }

    // Region tag matches centroid-radius band exactly.
    let mut a_core = 0.0;
    let mut a_clad = 0.0;
    let mut a_pml = 0.0;
    for (tri, &tag) in mesh.tris.iter().zip(tags.iter()) {
        let p = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
        let cy = (p[0][1] + p[1][1] + p[2][1]) / 3.0;
        let r = (cx * cx + cy * cy).sqrt();
        let expect = if r < core_r {
            REGION_CORE
        } else if r < clad_r {
            REGION_CLADDING
        } else {
            REGION_PML
        };
        assert_eq!(tag, expect, "tag mismatch at r={r}");
        // area
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1]];
        let area = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();
        match tag {
            REGION_CORE => a_core += area,
            REGION_CLADDING => a_clad += area,
            REGION_PML => a_pml += area,
            _ => unreachable!(),
        }
    }
    // All three regions present.
    assert!(a_core > 0.0 && a_clad > 0.0 && a_pml > 0.0);
    // PML is the OUTER band: every PML centroid radius ≥ clad_r, and the
    // PML area is close to the exact annulus area π(outer² − clad²) (the
    // polygonal mesh slightly under-fills the circle).
    let exact_pml = std::f64::consts::PI * (outer_r * outer_r - clad_r * clad_r);
    let frac = a_pml / exact_pml;
    assert!(
        (0.90..=1.0).contains(&frac),
        "PML area fraction of exact annulus = {frac} (expected ~1)"
    );
}

/// Stretch tensor: identity for r ≤ r_pml_inner; in the annulus Λ_t and
/// Λ_t⁻¹ are mutual inverses; the radial eigenvalue is 1/s and transverse
/// is s; complex entries appear only in the PML.
#[test]
fn pml_stretch_tensor_2d_inverse_and_identity() {
    let r_in = 2.0;
    let r_out = 3.0;
    let sigma_0 = 5.0;

    // Interior point: identity, real, curl_weight = 1.
    let (lam, cw) = pml_stretch_tensor_2d([1.0, 0.5], r_in, r_out, sigma_0);
    assert_eq!(cw, c64::new(1.0, 0.0));
    assert_eq!(lam[0][0], c64::new(1.0, 0.0));
    assert_eq!(lam[1][1], c64::new(1.0, 0.0));
    assert_eq!(lam[0][1], c64::new(0.0, 0.0));
    assert_eq!(lam[1][0], c64::new(0.0, 0.0));

    // sigma_0 = 0 → identity even in the annulus.
    let (lam0, cw0) = pml_stretch_tensor_2d([2.5, 0.0], r_in, r_out, 0.0);
    assert_eq!(cw0, c64::new(1.0, 0.0));
    assert_eq!(lam0[0][0], c64::new(1.0, 0.0));
    assert_eq!(lam0[0][1], c64::new(0.0, 0.0));

    // Annulus point on +x axis (r̂ = x̂): Λ_t = diag(1/s, s); complex.
    let r = 2.5;
    let (lam_a, cw_a) = pml_stretch_tensor_2d([r, 0.0], r_in, r_out, sigma_0);
    let u = (r - r_in) / (r_out - r_in);
    let s = c64::new(1.0, -sigma_0 * u * u);
    let s_inv = c64::new(1.0, 0.0) / s;
    // radial (xx) eigenvalue = 1/s, transverse (yy) = s.
    let close = |a: c64, b: c64| (a - b).norm() < 1e-12;
    assert!(close(lam_a[0][0], s_inv), "Λ_xx should be 1/s");
    assert!(close(lam_a[1][1], s), "Λ_yy should be s");
    assert!(close(lam_a[0][1], c64::new(0.0, 0.0)));
    assert!(close(cw_a, s_inv), "curl weight should be 1/s");
    assert!(lam_a[0][0].im != 0.0, "complex in PML");

    // Inverse consistency at an off-axis annulus point: Λ_t · Λ_t⁻¹ = I.
    let (lam_b, _) = pml_stretch_tensor_2d([1.8, 1.8], r_in, r_out, sigma_0);
    // Build Λ_t⁻¹ analytically: same construction with s↔1/s.
    let rr = (1.8_f64 * 1.8 + 1.8 * 1.8).sqrt();
    let ub = ((rr - r_in) / (r_out - r_in)).clamp(0.0, 1.0);
    let sb = c64::new(1.0, -sigma_0 * ub * ub);
    let sb_inv = c64::new(1.0, 0.0) / sb;
    let rx = 1.8 / rr;
    let ry = 1.8 / rr;
    let coeff_inv = sb - sb_inv; // (s − 1/s) for the inverse tensor
    let lam_inv = [
        [
            sb_inv + coeff_inv * c64::new(rx * rx, 0.0),
            coeff_inv * c64::new(rx * ry, 0.0),
        ],
        [
            coeff_inv * c64::new(ry * rx, 0.0),
            sb_inv + coeff_inv * c64::new(ry * ry, 0.0),
        ],
    ];
    // product = Λ_t · Λ_inv
    #[allow(clippy::needless_range_loop)] // explicit i,j,k matrix-product indices
    for a in 0..2 {
        for b in 0..2 {
            let mut acc = c64::new(0.0, 0.0);
            for kk in 0..2 {
                acc += lam_b[a][kk] * lam_inv[kk][b];
            }
            let want = if a == b {
                c64::new(1.0, 0.0)
            } else {
                c64::new(0.0, 0.0)
            };
            assert!(
                (acc - want).norm() < 1e-10,
                "Λ·Λ⁻¹ entry ({a},{b}) = {acc}, want {want}"
            );
        }
    }
}

/// LOAD-BEARING: with sigma_0 = 0 (and even with PML-tagged triangles
/// present), the complex UPML assembly equals the real
/// `assemble_2d_nedelec2_sparse_interior` output embedded in c64 with zero
/// imaginary part — entry for entry. Proves the complex path does not
/// corrupt the validated real assembly.
#[test]
fn pml_assembly_sigma0_reduces_to_real_bit_for_bit() {
    let core_r = 1.0;
    let clad_r = 2.0;
    let outer_r = 3.0;
    let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 3, 18);
    // Dielectric ε_r: core 2.1, cladding 1.0; PML carries cladding ε_r.
    let eps_r: Vec<f64> = tags
        .iter()
        .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
        .collect();
    let mask = disk_pec_interior_dofs2(&mesh, outer_r);

    let real = assemble_2d_nedelec2_sparse_interior(&mesh, &eps_r, &mask).unwrap();
    // sigma_0 = 0 with PML tags PRESENT (so the PML branch executes but
    // produces identity tensors).
    let cplx =
        assemble_2d_nedelec2_pml_sparse_interior(&mesh, &eps_r, &tags, &mask, clad_r, outer_r, 0.0)
            .unwrap();

    assert_eq!(cplx.dim, real.dim);
    let n = real.dim;
    for c in 0..n {
        for r in 0..n {
            for (cm, rm, name) in [
                (&cplx.k, &real.k, "K"),
                (&cplx.m_eps, &real.m_eps, "M_eps"),
                (&cplx.m1, &real.m1, "M1"),
            ] {
                let cv = c64_entry(cm, r, c);
                let rv = f64_entry(rm, r, c);
                assert_eq!(cv.im, 0.0, "{name}({r},{c}) imag must be 0, got {cv}");
                assert_eq!(
                    cv.re, rv,
                    "{name}({r},{c}) real mismatch: complex {} vs real {rv}",
                    cv.re
                );
            }
        }
    }
}

/// With sigma_0 > 0 the PML annulus carries complex entries; the
/// non-PML (core/cladding) block stays real; and K, M_eps, M1 are all
/// complex-SYMMETRIC (bilinear, not Hermitian) — A == Aᵀ.
#[test]
fn pml_assembly_complex_symmetric_and_localized() {
    let core_r = 1.0;
    let clad_r = 2.0;
    let outer_r = 3.0;
    let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 3, 18);
    let eps_r: Vec<f64> = tags
        .iter()
        .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
        .collect();
    let mask = disk_pec_interior_dofs2(&mesh, outer_r);

    let cplx =
        assemble_2d_nedelec2_pml_sparse_interior(&mesh, &eps_r, &tags, &mask, clad_r, outer_r, 8.0)
            .unwrap();
    let n = cplx.dim;

    // Complex-symmetric: A(r,c) == A(c,r) for K, M_eps, M1.
    let mut any_complex = false;
    for c in 0..n {
        for r in 0..n {
            for a in [&cplx.k, &cplx.m_eps, &cplx.m1] {
                let v = c64_entry(a, r, c);
                let vt = c64_entry(a, c, r);
                assert!(
                    (v - vt).norm() < 1e-12,
                    "complex-symmetry: ({r},{c})={v} vs ({c},{r})={vt}"
                );
                if v.im.abs() > 1e-14 {
                    any_complex = true;
                }
            }
        }
    }
    assert!(
        any_complex,
        "sigma_0 > 0 must introduce complex entries from the PML annulus"
    );
}

// ---- PML-B (#332): complex-pencil modal solve + clean LP01 ----------

/// Build the SMF-28-like PML problem: per-triangle ε_r (core n_core²,
/// cladding+PML n_clad²), the PEC mask, and the geometry.
#[allow(clippy::type_complexity)]
fn smf28_pml_problem(
    clad_mult: f64,
    pml_mult: f64,
    n_radial: usize,
    n_angular: usize,
) -> (TriMesh, Vec<i32>, Vec<f64>, Vec<bool>, f64, f64, f64) {
    const N_CORE: f64 = 1.4504;
    const N_CLAD: f64 = 1.4447;
    const A_UM: f64 = 4.1;
    const LAMBDA_UM: f64 = 1.55;
    let clad_r = clad_mult * A_UM;
    let outer_r = pml_mult * A_UM;
    let (mesh, tags) = disk_tri_mesh_pml(A_UM, clad_r, outer_r, n_radial, n_angular);
    let eps_r = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            N_CORE * N_CORE
        } else {
            N_CLAD * N_CLAD
        }
    });
    let mask = disk_pec_interior_dofs2(&mesh, outer_r);
    let k0 = 2.0 * std::f64::consts::PI / LAMBDA_UM;
    (mesh, tags, eps_r, mask, clad_r, outer_r, k0)
}

/// **The headline PML-B test (#332).** With the cladding absorbed by a
/// 2D UPML, the genuine weakly-guiding LP₀₁ of the SMF-28 fiber
/// isolates cleanly — the thing the far PEC wall could not do (#329,
/// best core-energy fraction only 0.34–0.49). We assert the selected
/// fundamental is genuinely core-confined (core-energy fraction ≳0.8),
/// has `Re(n_eff)` inside the index window `(n_clad, n_core)`, and is
/// genuinely bound (`|Im(β²)|` tiny). We do **not** yet assert ≤1% b
/// convergence — that is PML-C's convergence study; here we only show
/// the mode is cleanly isolated.
#[test]
fn pml_smf28_isolates_clean_lp01() {
    const N_CORE: f64 = 1.4504;
    const N_CLAD: f64 = 1.4447;
    let (mesh, tags, eps_r, mask, clad_r, outer_r, k0) = smf28_pml_problem(8.0, 11.0, 5, 60);
    let sigma_0 = 6.0;

    let modes =
        solve_dielectric_modes2_pml(&mesh, &eps_r, &tags, &mask, clad_r, outer_r, sigma_0, k0, 1)
            .expect("PML modal solve must succeed");
    assert!(
        !modes.is_empty(),
        "PML solve must return a guided LP01 (got none)"
    );
    let m = &modes[0];

    // Re(n_eff) strictly inside the weakly-guiding window.
    assert!(
        m.n_eff.re > N_CLAD && m.n_eff.re < N_CORE,
        "Re(n_eff)={} must lie in ({N_CLAD}, {N_CORE})",
        m.n_eff.re
    );

    // Genuinely bound: |Im(β²)| negligible (the PML adds no spurious
    // loss to a truly trapped mode).
    let rel_im = m.beta_sq.im.abs() / m.beta_sq.re.abs().max(1.0);
    assert!(
        rel_im < 1e-6,
        "guided LP01 must be genuinely bound: |Im(β²)|/Re(β²)={rel_im:.3e} (β²={})",
        m.beta_sq
    );

    // The payoff: core-energy fraction confirms genuine LP01, NOT a box
    // mode. PEC-era best was 0.34–0.49; a clean fundamental is ≳0.8.
    let shape = dielectric_mode_field_shape_pml(&mesh, &tags, m);
    assert!(
        shape.core_energy_fraction >= 0.8,
        "core-energy fraction {:.4} must be ≳0.8 (clean LP01); PEC-era best was 0.34–0.49",
        shape.core_energy_fraction
    );

    eprintln!(
        "pml_smf28_isolates_clean_lp01: Re(n_eff)={:.6}, Im(n_eff)={:.3e}, \
         |Im(β²)|={:.3e}, core_energy_fraction={:.4}",
        m.n_eff.re,
        m.n_eff.im,
        m.beta_sq.im.abs(),
        shape.core_energy_fraction
    );
}

/// With `sigma_0 = 0` the complex PML assembly reduces bit-for-bit to
/// the real path (proven in
/// `pml_assembly_sigma0_reduces_to_real_bit_for_bit`), so the complex
/// pencil eigensolve must reproduce the **real** dielectric spectrum
/// embedded in `c64`: every recovered β² is real (`Im ≈ 0`), and the
/// σ₀=0 PML-selected mode's β² must coincide with one of the real-path
/// `dielectric_raw_candidates_p2` eigenvalues on the same mesh.
///
/// (We compare against the *raw* real spectrum rather than the filtered
/// `solve_dielectric_modes2` output: the two entry points apply
/// different ceilings and selection rules — the real PEC path uses the
/// strip-slab ceiling + largest-β, the PML path the n_core ceiling +
/// lowest-order-bound — so their *selected* modes legitimately differ.
/// The load-bearing claim is the spectral equivalence, which this
/// checks directly.)
#[test]
fn pml_sigma0_matches_real_solve() {
    // A modest higher-contrast disk (uniform region map: core ε=2.1,
    // else 1.0).
    let core_r = 1.0;
    let clad_r = 2.0;
    let outer_r = 3.0;
    let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, 4, 24);
    let eps_r: Vec<f64> = tags
        .iter()
        .map(|&t| if t == REGION_CORE { 2.1 } else { 1.0 })
        .collect();
    let mask = disk_pec_interior_dofs2(&mesh, outer_r);
    let k0 = 2.0;

    // Complex PML solve with sigma_0 = 0 (transparent layer).
    let pml_modes =
        solve_dielectric_modes2_pml(&mesh, &eps_r, &tags, &mask, clad_r, outer_r, 0.0, k0, 1)
            .expect("σ₀=0 PML solve must succeed");
    assert!(!pml_modes.is_empty(), "σ₀=0 PML solve must return a mode");
    let pm = &pml_modes[0];

    // (a) σ₀=0 ⇒ real β² (PML off ⇒ real spectrum embedded in c64).
    assert!(
        pm.beta_sq.im.abs() < 1e-6 * pm.beta_sq.re.abs().max(1.0),
        "σ₀=0 must give a real β²: Im={:.3e}",
        pm.beta_sq.im
    );

    // (b) The PML β² must coincide with a REAL-path eigenvalue — the
    // complex path reproduces the validated real spectrum.
    let real_cands = dielectric_raw_candidates_p2(&mesh, &eps_r, &mask, k0, 32, None)
        .expect("real raw candidates must succeed")
        .cands;
    let best = real_cands
        .iter()
        .map(|c| (c.beta_sq - pm.beta_sq.re).abs())
        .fold(f64::INFINITY, f64::min);
    let rel = best / pm.beta_sq.re.abs().max(1e-12);
    assert!(
        rel < 1e-6,
        "σ₀=0 PML β²={:.8} must match a real-path eigenvalue (closest abs err {best:.3e}, rel {rel:.3e})",
        pm.beta_sq.re
    );
}

// ===================================================================
// Graded / non-uniform mesh strategies (issue #337)
// ===================================================================

/// Sorted unique ring radii of a concentric disk mesh: every distinct
/// node radius, ascending (includes 0 for the center).
fn ring_radii(mesh: &TriMesh) -> Vec<f64> {
    let mut rs: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt())
        .collect();
    rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    rs.dedup_by(|a, b| (*a - *b).abs() < 1e-12 * (a.abs() + 1.0));
    rs
}

#[test]
fn graded_uniform_reproduces_disk_tri_mesh_bit_for_bit() {
    // The load-bearing guarantee: Uniform grading == the original mesher,
    // node-for-node, tri-for-tri, tag-for-tag (exact f64 equality).
    for &(cr, or, nr, na) in &[
        (1.0, 3.0, 2, 8),
        (0.6, 2.5, 4, 24),
        (1.0, 10.0, 5, 31),
        (2.0, 7.0, 1, 5),
    ] {
        let (m0, t0) = disk_tri_mesh(cr, or, nr, na);
        let (m1, t1) = disk_tri_mesh_graded(
            cr,
            or,
            nr,
            na,
            RadialGrading::Uniform,
            RadialGrading::Uniform,
        );
        assert_eq!(m0.nodes, m1.nodes, "node coords must match bit-for-bit");
        assert_eq!(m0.tris, m1.tris, "triangles must match bit-for-bit");
        assert_eq!(t0, t1, "region tags must match bit-for-bit");
    }
}

#[test]
fn graded_uniform_reproduces_disk_tri_mesh_pml_bit_for_bit() {
    for &(cr, cl, or, nr, na) in &[
        (1.0, 2.0, 3.0, 4, 24),
        (0.5, 1.7, 4.0, 3, 18),
        (1.0, 5.0, 10.0, 6, 37),
    ] {
        let (m0, t0) = disk_tri_mesh_pml(cr, cl, or, nr, na);
        let (m1, t1) = disk_tri_mesh_pml_graded(
            cr,
            cl,
            or,
            nr,
            na,
            RadialGrading::Uniform,
            RadialGrading::Uniform,
            RadialGrading::Uniform,
        );
        assert_eq!(m0.nodes, m1.nodes);
        assert_eq!(m0.tris, m1.tris);
        assert_eq!(t0, t1);
    }
}

#[test]
fn graded_uniform_reproduces_rect_tri_mesh_bit_for_bit() {
    for &(nx, ny, w, h) in &[
        (3usize, 2usize, 1.0, 1.0),
        (5, 7, 2.0, 0.5),
        (1, 1, 3.0, 4.0),
    ] {
        let m0 = rect_tri_mesh(nx, ny, w, h);
        let m1 = rect_tri_mesh_graded(nx, ny, w, h, RadialGrading::Uniform, RadialGrading::Uniform);
        assert_eq!(m0.nodes, m1.nodes);
        assert_eq!(m0.tris, m1.tris);
    }
}

#[test]
fn graded_meshes_are_valid_ccw_and_nondegenerate() {
    // Each strategy must yield a connected mesh of strictly-positive-area
    // CCW triangles. Reuse the existing connectivity/CCW machinery.
    let gradings = [
        RadialGrading::Uniform,
        RadialGrading::Geometric { ratio: 1.4 },
        RadialGrading::Geometric { ratio: 0.7 },
        RadialGrading::Linear { ratio: 2.5 },
        RadialGrading::InterfaceClustered { strength: 1.5 },
    ];
    for g in gradings {
        let (mesh, tags) = disk_tri_mesh_graded(1.0, 3.0, 5, 31, g, g);
        assert_eq!(tags.len(), mesh.n_tris());
        let mut min_area = f64::INFINITY;
        for t in &mesh.tris {
            let a = signed_area(&mesh, t);
            assert!(a > 0.0, "non-CCW triangle {t:?} (area {a}) under {g:?}");
            min_area = min_area.min(a);
        }
        assert!(min_area > 1e-12, "degenerate triangle under {g:?}");
        // Ring radii strictly increasing (no collapsed / out-of-order ring).
        let rs = ring_radii(&mesh);
        for w in rs.windows(2) {
            assert!(w[1] > w[0], "non-monotonic ring radii under {g:?}");
        }
        // A ring boundary still lands exactly on the core radius.
        assert!(
            rs.iter().any(|&r| (r - 1.0).abs() < 1e-9),
            "core-radius ring missing under {g:?}"
        );
    }
}

#[test]
fn graded_region_tags_and_area_fraction_correct_under_grading() {
    // The core area fraction must still match π·a²/π·R² = (a/R)²
    // regardless of how rings are distributed within a region.
    let cr = 1.0_f64;
    let or = 3.0_f64;
    let expected = (cr / or).powi(2);
    for g in [
        RadialGrading::Geometric { ratio: 1.5 },
        RadialGrading::Linear { ratio: 0.5 },
        RadialGrading::InterfaceClustered { strength: 2.0 },
    ] {
        let (mesh, tags) = disk_tri_mesh_graded(cr, or, 8, 64, g, g);
        let mut a_core = 0.0;
        let mut a_tot = 0.0;
        for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
            let area = signed_area(&mesh, t).abs();
            a_tot += area;
            if tag == REGION_CORE {
                a_core += area;
            }
            // Tag must agree with the centroid-radius band.
            let p = [
                mesh.nodes[t[0] as usize],
                mesh.nodes[t[1] as usize],
                mesh.nodes[t[2] as usize],
            ];
            let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
            let cy = (p[0][1] + p[1][1] + p[2][1]) / 3.0;
            let r = (cx * cx + cy * cy).sqrt();
            let expect = if r < cr { REGION_CORE } else { REGION_CLADDING };
            assert_eq!(tag, expect, "tag/centroid mismatch under {g:?}");
        }
        let frac = a_core / a_tot;
        // Polygonal approximation to the circle ⇒ a few-% tolerance.
        assert!(
            (frac - expected).abs() < 0.02,
            "core area fraction {frac:.4} vs expected {expected:.4} under {g:?}"
        );
    }
}

#[test]
fn graded_boundary_node_set_unchanged_under_grading() {
    // The outer boundary node set keys on geometry (r ≈ outer_radius),
    // so grading must not change it: still exactly the outer ring.
    let or = 3.0;
    for g in [
        RadialGrading::Geometric { ratio: 1.6 },
        RadialGrading::InterfaceClustered { strength: 1.0 },
    ] {
        let (mesh, _) = disk_tri_mesh_graded(1.0, or, 5, 24, g, g);
        let bnd = disk_boundary_nodes(&mesh, or);
        let n_bnd = bnd.iter().filter(|&&b| b).count();
        assert_eq!(
            n_bnd, 24,
            "boundary node count must be n_angular under {g:?}"
        );
        // p=2 interior-DOF mask still builds (same edge topology).
        let mask = disk_pec_interior_dofs2(&mesh, or);
        assert_eq!(mask.len(), 2 * mesh.edges().len() + 2 * mesh.n_tris());
    }
}

#[test]
fn geometric_grading_step_ratio_matches_configured_factor() {
    // Adjacent radial steps inside a region must scale by `ratio`.
    let ratio = 1.5;
    // Single region (set core tiny so the cladding band dominates the
    // ring count we measure); inspect the cladding band's steps.
    let (mesh, _) = disk_tri_mesh_graded(
        1.0,
        5.0,
        8,
        48,
        RadialGrading::Uniform,
        RadialGrading::Geometric { ratio },
    );
    let rs = ring_radii(&mesh);
    // Cladding rings are the radii strictly greater than 1.0 (core edge).
    let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
    let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
        .chain(clad.windows(2).map(|w| w[1] - w[0]))
        .collect();
    for w in steps.windows(2) {
        let got = w[1] / w[0];
        assert!(
            (got - ratio).abs() < 1e-9,
            "geometric step ratio {got} ≠ configured {ratio}"
        );
    }
}

#[test]
fn linear_grading_last_step_is_ratio_times_first() {
    let ratio = 3.0;
    let (mesh, _) = disk_tri_mesh_graded(
        1.0,
        5.0,
        8,
        48,
        RadialGrading::Uniform,
        RadialGrading::Linear { ratio },
    );
    let rs = ring_radii(&mesh);
    let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
    let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
        .chain(clad.windows(2).map(|w| w[1] - w[0]))
        .collect();
    let got = steps[steps.len() - 1] / steps[0];
    assert!(
        (got - ratio).abs() < 1e-9,
        "linear last/first step ratio {got} ≠ configured {ratio}"
    );
    // And the steps grow monotonically (ratio > 1).
    for w in steps.windows(2) {
        assert!(w[1] > w[0], "linear steps must increase for ratio > 1");
    }
}

#[test]
fn interface_clustered_min_step_is_adjacent_to_interface() {
    // Cladding clustered toward its inner edge (r = a): the smallest
    // cladding step must be the first one (adjacent to r = a).
    let (mesh, _) = disk_tri_mesh_graded(
        1.0,
        5.0,
        8,
        48,
        RadialGrading::Uniform,
        RadialGrading::InterfaceClustered { strength: 3.0 },
    );
    let rs = ring_radii(&mesh);
    let clad: Vec<f64> = rs.into_iter().filter(|&r| r > 1.0 + 1e-9).collect();
    let steps: Vec<f64> = std::iter::once(clad[0] - 1.0)
        .chain(clad.windows(2).map(|w| w[1] - w[0]))
        .collect();
    let min_idx = steps
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(
        min_idx, 0,
        "interface-clustered min step must be adjacent to r=a; steps={steps:?}"
    );

    // Core clustered toward its outer edge (r = a): the smallest core
    // step must be the *last* one (adjacent to r = a).
    let (mesh2, _) = disk_tri_mesh_graded(
        1.0,
        5.0,
        8,
        48,
        RadialGrading::InterfaceClustered { strength: 3.0 },
        RadialGrading::Uniform,
    );
    let rs2 = ring_radii(&mesh2);
    let core: Vec<f64> = rs2.into_iter().filter(|&r| r <= 1.0 + 1e-9).collect();
    // core includes 0.0 (center) then the core rings up to 1.0.
    let csteps: Vec<f64> = core.windows(2).map(|w| w[1] - w[0]).collect();
    let cmin_idx = csteps
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(
        cmin_idx,
        csteps.len() - 1,
        "core interface-clustered min step must be the outermost (at r=a); steps={csteps:?}"
    );
}

#[test]
fn pml_graded_supports_per_band_grading_and_conforms() {
    // The downstream graded-fiber experiment needs the PML mesher with
    // independent per-band grading; all three band boundaries stay fixed.
    let (mesh, tags) = disk_tri_mesh_pml_graded(
        1.0,
        2.0,
        3.0,
        6,
        36,
        RadialGrading::InterfaceClustered { strength: 2.0 },
        RadialGrading::Geometric { ratio: 1.3 },
        RadialGrading::Linear { ratio: 0.5 },
    );
    assert_eq!(tags.len(), mesh.n_tris());
    for t in &mesh.tris {
        assert!(signed_area(&mesh, t) > 0.0, "non-CCW PML graded triangle");
    }
    let rs = ring_radii(&mesh);
    for &b in &[1.0, 2.0, 3.0] {
        assert!(
            rs.iter().any(|&r| (r - b).abs() < 1e-9),
            "band boundary {b} not conformed under graded PML"
        );
    }
    // Tags present for all three regions.
    assert!(tags.contains(&REGION_CORE));
    assert!(tags.contains(&REGION_CLADDING));
    assert!(tags.contains(&REGION_PML));
}

#[test]
fn worst_aspect_ratio_matches_uniform_disk_quality() {
    // Sanity: on the documented balanced-knob uniform mesh the exposed
    // worst aspect ratio agrees with the test helper / stays < 7.
    let (mesh, _) = disk_tri_mesh(1.0, 3.0, 4, 25);
    let exposed = worst_aspect_ratio(&mesh);
    let mut by_helper = 0.0_f64;
    for t in &mesh.tris {
        by_helper = by_helper.max(aspect_ratio(&mesh, t));
    }
    assert!((exposed - by_helper).abs() < 1e-9);
    assert!(exposed < 7.0, "uniform worst aspect {exposed} exceeded 7");
}

#[test]
fn aspect_ratio_guard_accepts_mild_and_rejects_sliver_grading() {
    // Mild grading passes the checked constructor.
    let ok = disk_tri_mesh_graded_checked(
        1.0,
        3.0,
        5,
        31,
        RadialGrading::Geometric { ratio: 1.2 },
        RadialGrading::Geometric { ratio: 1.2 },
        ASPECT_RATIO_SLIVER_BOUND,
    );
    assert!(ok.is_ok(), "mild grading must pass the aspect guard");

    // Deliberately pathological grading (extreme geometric ratio with few
    // angular sectors) manufactures slivers and must be rejected.
    let sliver = disk_tri_mesh_graded_checked(
        1.0,
        3.0,
        12,
        6,
        RadialGrading::Geometric { ratio: 3.0 },
        RadialGrading::Geometric { ratio: 3.0 },
        ASPECT_RATIO_SLIVER_BOUND,
    );
    assert!(
        sliver.is_err(),
        "strong grading must be rejected by the aspect guard; worst aspect was {}",
        worst_aspect_ratio(
            &disk_tri_mesh_graded(
                1.0,
                3.0,
                12,
                6,
                RadialGrading::Geometric { ratio: 3.0 },
                RadialGrading::Geometric { ratio: 3.0 },
            )
            .0
        )
    );

    // PML checked constructor likewise guards.
    let pml_ok = disk_tri_mesh_pml_graded_checked(
        1.0,
        2.0,
        3.0,
        5,
        31,
        RadialGrading::Uniform,
        RadialGrading::Geometric { ratio: 1.2 },
        RadialGrading::Uniform,
        ASPECT_RATIO_SLIVER_BOUND,
    );
    assert!(pml_ok.is_ok());
}

// ----- Selection-level hole check (issue #850) ----------------------

/// The localized withheld pairs measured on the default-tier
/// high-contrast PML fixture (`high_contrast_fiber_benchmark`, mesh
/// `(4, 48)`, `n_modes = 4`): `(Re β², Im β², ρ)`.
const HC_PML_WITHHELD_850: [(f64, f64, f64); 7] = [
    (34.93929, 5.98e-16, 1.37e-7),
    (34.88800, -2.00e-8, 4.15e-4),
    (34.75005, -4.65e-13, 4.03e-8),
    (34.72720, -1.18e-12, 1.80e-6),
    (34.70756, -7.99e-11, 4.60e-5),
    (34.69061, 1.57e-8, 6.78e-4),
    (34.67594, 1.34e-6, 4.91e-3),
];

/// A withheld candidate with an explicit curl ratio (issue #916).
fn withheld_916(beta_sq: c64, residual: f64, curl_ratio: f64) -> WithheldCandidate {
    WithheldCandidate {
        beta_sq,
        residual,
        curl_ratio,
        eps_weighted: None,
    }
}

/// Hole rule of a classifier whose own curl floor is `curl_floor`, with
/// the empty-set threshold at its margin over that floor and no
/// Rayleigh test (issue #913). The #850 / #916 tests, which all pass a
/// reference mode, depend only on `curl_floor`.
fn rule_913(curl_floor: f64) -> HoleRule {
    HoleRule {
        curl_floor,
        empty_set_curl_floor: EMPTY_SET_CURL_MARGIN * curl_floor,
        rayleigh: None,
    }
}

/// Curl ratio well inside the physical band, for the #850 rule tests
/// that exercise the other conditions. The recorded #850 pairs carry no
/// curl ratio (the check had none then).
const PHYSICAL_CURL_916: f64 = 1e-2;

fn hc_pml_withheld_850() -> Vec<WithheldCandidate> {
    HC_PML_WITHHELD_850
        .iter()
        .map(|&(re, im, r)| withheld_916(c64::new(re, im), r, PHYSICAL_CURL_916))
        .collect()
}

/// Negative control (issue #850): the `β² = 34.94` pair the high-contrast
/// fixture withholds is localized and bound-like, but it lies *below*
/// the lowest returned bound mode (`35.4753`), so it is not a hole. The
/// same pair *is* a hole when the returned set reaches below it.
#[test]
fn selection_hole_negative_control_34_94_is_not_a_hole() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let window = (1.4447_f64.powi(2) * k0 * k0, 1.4874_f64.powi(2) * k0 * k0);
    let sigma = window.1 * (1.0 - 1e-3);
    let withheld = hc_pml_withheld_850();
    let floor = rule_913(physical_curl_floor_pml());
    assert_eq!(
        selection_hole(&withheld, sigma, window, Some(35.475_30), &floor),
        None,
        "the 34.94 pair below the lowest returned bound mode must not trip"
    );
    // A returned set reaching down to 34.5 would skip it: hole, and the
    // highest qualifying pair (34.939) is the one reported. 34.888 has
    // |Im| / Re = 5.7e-10 (bound-like) but sits below 34.939.
    let hole = selection_hole(&withheld, sigma, window, Some(34.5), &floor)
        .expect("a localized bound-like pair above the reference is a hole");
    assert_eq!(hole.beta_sq.re, 34.93929);
    assert_eq!(hole.residual, 1.37e-7);
}

/// Each condition of the hole rule is necessary (issue #850).
#[test]
fn selection_hole_requires_window_bound_like_and_certain_margin() {
    let window = (34.0, 36.0);
    let sigma = 35.96;
    let lowest = Some(35.0);
    let hole = |l: c64, r: f64| {
        selection_hole(
            &[withheld_916(l, r, PHYSICAL_CURL_916)],
            sigma,
            window,
            lowest,
            &rule_913(physical_curl_floor_pml()),
        )
    };
    // Baseline: in window, bound-like, certainly above 35.0.
    assert!(hole(c64::new(35.5, 0.0), 1e-6).is_some());
    // Leaky (|Im| / Re = 1e-6 > 1e-8): not bound-like.
    assert!(hole(c64::new(35.5, 35.5e-6), 1e-6).is_none());
    // Outside the window.
    assert!(hole(c64::new(36.2, 0.0), 1e-9).is_none());
    // At the ceiling to rounding (the gradient cluster at ε_core k₀²):
    // its uncertainty straddles the ceiling.
    assert!(hole(c64::new(36.0 - 1e-14, 0.0), 6e-8).is_none());
    // Above the reference by less than its own uncertainty
    // (w = 1e-3 · 35.96 ≈ 0.036 > 0.01): possibly the same eigenvalue.
    assert!(hole(c64::new(35.01, 0.0), 1e-3).is_none());
    // Same pair with a tight residual is certainly above: hole.
    assert!(hole(c64::new(35.01, 0.0), 1e-6).is_some());
}

/// Issue #916: a localized withheld pair above the reference that
/// would be a hole on every other count is **not** one when its Ritz
/// vector is curl-free (curl ratio at or below the classifier's floor):
/// the classifier would have dropped it as gradient nullspace, so the
/// selection is complete. The same pair with curl above the floor is a
/// hole. Checked against each classifier's own floor.
#[test]
fn selection_hole_skips_gradient_pairs_below_curl_floor() {
    let window = (34.0, 36.0);
    let sigma = 35.96;
    let lowest = Some(35.0);
    let pair = c64::new(35.5, 0.0);
    // (floor function, its value): the PEC p=1 / p=2 floors at the
    // ~3 %-step graddiv fiber contrast (1.4874² / 1.4447²) and the PML
    // floor.
    let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
    let floors = [
        physical_curl_floor(e_core, e_clad),
        physical_curl_floor_p2(e_core, e_clad),
        physical_curl_floor_pml(),
    ];
    for floor in floors {
        let hole = |curl: f64| {
            selection_hole(
                &[withheld_916(pair, 1e-6, curl)],
                sigma,
                window,
                lowest,
                &rule_913(floor),
            )
        };
        // Curl-free gradient pair (f64 noise) and a pair exactly at the
        // floor: never a hole.
        assert_eq!(
            hole(1e-13),
            None,
            "gradient pair tripped (floor {floor:.2e})"
        );
        assert_eq!(hole(floor), None, "pair at the floor tripped ({floor:.2e})");
        // Just above the floor: a hole.
        let hit = hole(floor * 1.01).expect("curl-bearing pair above the floor is a hole");
        assert_eq!(hit.beta_sq, pair);
    }
    // The #850 graddiv twin: curl 2.9e-3 against its 6.0e-4 floor
    // (p=1 floor at that contrast) is still a hole.
    let graddiv_floor = physical_curl_floor(e_core, e_clad);
    assert!(
        (graddiv_floor - 6.0e-4).abs() < 0.05e-4,
        "graddiv floor {graddiv_floor:.3e}"
    );
    assert!(
        selection_hole(
            &[withheld_916(pair, 4.4e-5, 2.9e-3)],
            sigma,
            window,
            lowest,
            &rule_913(graddiv_floor)
        )
        .is_some()
    );
    // A gradient pair higher in the window does not mask the physical
    // hole below it: the reported hole is the curl-bearing pair.
    let both = [
        withheld_916(c64::new(35.8, 0.0), 1e-7, 1e-13),
        withheld_916(pair, 1e-6, 1e-2),
    ];
    let hit = selection_hole(&both, sigma, window, lowest, &rule_913(graddiv_floor))
        .expect("the curl-bearing pair is a hole");
    assert_eq!(hit.beta_sq, pair);
    // A gradient pair alone above the reference: no error from the
    // typed check either.
    assert!(
        check_selection_hole(
            "solve_dielectric_modes",
            &both[..1],
            sigma,
            window,
            lowest,
            &rule_913(graddiv_floor),
            144,
        )
        .is_ok()
    );
}

/// A hand-built analytic-cladding checked result (issue #934): no
/// converged pairs, and each `(λ, ρ, curl)` of `withheld` recorded as a
/// localized withheld pair whose one-entry Ritz vector carries `curl`
/// (read back by [`curl_from_vector_934`]). `spurious` adds non-localized
/// entries to `rejected` only, as the solver records them.
fn checked_934(withheld: &[(c64, f64, f64)], spurious: &[(c64, f64)]) -> CheckedComplexEigenpairs {
    use crate::eigen::complex::{ComplexEigenPair, ComplexWithheldEigenpair};
    let mut rejected: Vec<(c64, f64)> = withheld.iter().map(|&(l, r, _)| (l, r)).collect();
    rejected.extend_from_slice(spurious);
    CheckedComplexEigenpairs {
        pairs: Vec::new(),
        residuals: Vec::new(),
        rejected,
        localized_withheld: withheld
            .iter()
            .map(|&(lambda, residual, curl)| ComplexWithheldEigenpair {
                pair: ComplexEigenPair {
                    lambda,
                    vector: vec![c64::new(curl, 0.0)],
                },
                residual,
            })
            .collect(),
        screened: Vec::new(),
        requested: 4,
        lanczos_steps: 40,
        extended: true,
    }
}

/// Curl ratio "computed" from a [`checked_934`] Ritz vector.
fn curl_from_vector_934(x: &[c64]) -> f64 {
    x[0].re
}

/// The analytic-cladding geometry of the #934 tests: guided window
/// `(34, 36)`, `σ` just under the ceiling as the solve sets it, and a
/// selected fundamental at `β² = 35.5`.
fn geometry_934() -> ((f64, f64), f64, f64) {
    let window = (34.0, 36.0);
    let sigma = window.1 * (1.0 - 1e-3);
    let pick = 35.5_f64;
    (window, sigma, (pick - sigma).abs())
}

/// Issue #934: a localized withheld **gradient** pair (curl ratio at
/// f64 noise, or exactly at the floor) nearer `σ` than the selected
/// fundamental is not a hole of the analytic-cladding solve; it is
/// returned as skipped so the solve logs it. Before #934 the check was
/// curl-blind and failed the solve on it.
#[test]
fn analytic_cladding_hole_skips_gradient_pair_nearer_sigma() {
    let (window, sigma, reach) = geometry_934();
    let floor = physical_curl_floor_pml();
    for curl in [1e-13, floor] {
        let gradient = (c64::new(35.9, 0.0), 1e-7, curl);
        let checked = checked_934(&[gradient], &[]);
        // Precondition: the pair is a hole to the curl-blind rule.
        assert!(
            checked
                .localized_hole(sigma, reach, |l| l.re > window.0 && l.re < window.1)
                .is_some(),
            "fixture must be a curl-blind hole (curl {curl:.1e})"
        );
        let verdict =
            analytic_cladding_hole(&checked, sigma, reach, window, floor, curl_from_vector_934);
        assert_eq!(
            verdict.hole, None,
            "gradient pair (curl {curl:.1e}) tripped"
        );
        assert_eq!(verdict.skipped_low_curl.len(), 1);
        assert_eq!(verdict.skipped_low_curl[0].beta_sq, gradient.0);
        assert_eq!(verdict.skipped_low_curl[0].curl_ratio, curl);
    }
}

/// Issue #934: a curl-bearing withheld pair nearer `σ` than the pick is
/// still a hole, including when a gradient pair sits even nearer `σ`
/// (which is skipped, not reported, and does not mask it).
#[test]
fn analytic_cladding_hole_reports_physical_pair_nearer_sigma() {
    let (window, sigma, reach) = geometry_934();
    let floor = physical_curl_floor_pml();
    // SMF-28-like physical curl (1e-4) and one just above the floor.
    for curl in [1e-4, floor * 1.01] {
        let physical = (c64::new(35.8, 0.0), 2e-6, curl);
        let verdict = analytic_cladding_hole(
            &checked_934(&[physical], &[]),
            sigma,
            reach,
            window,
            floor,
            curl_from_vector_934,
        );
        let hole = verdict.hole.expect("curl-bearing pair nearer σ is a hole");
        assert_eq!(hole.beta_sq, physical.0);
        assert_eq!(hole.residual, physical.1);
        assert!(verdict.skipped_low_curl.is_empty());
    }
    let gradient = (c64::new(35.95, 0.0), 1e-7, 1e-13);
    let physical = (c64::new(35.8, 0.0), 2e-6, 1e-4);
    let verdict = analytic_cladding_hole(
        &checked_934(&[gradient, physical], &[]),
        sigma,
        reach,
        window,
        floor,
        curl_from_vector_934,
    );
    assert_eq!(verdict.hole.map(|h| h.beta_sq), Some(physical.0));
    assert_eq!(verdict.skipped_low_curl.len(), 1);
    assert_eq!(verdict.skipped_low_curl[0].beta_sq, gradient.0);
}

/// Issue #934: a curl-bearing withheld pair farther from `σ` than the
/// pick, or outside the guided window, is not a hole and is not listed
/// as skipped either.
#[test]
fn analytic_cladding_hole_ignores_pairs_farther_than_pick_or_out_of_window() {
    let (window, sigma, reach) = geometry_934();
    let floor = physical_curl_floor_pml();
    let farther = (c64::new(35.0, 0.0), 2e-6, 1e-4);
    let above_ceiling = (c64::new(36.01, 0.0), 1e-7, 1e-4);
    let verdict = analytic_cladding_hole(
        &checked_934(&[farther, above_ceiling], &[]),
        sigma,
        reach,
        window,
        floor,
        curl_from_vector_934,
    );
    assert_eq!(verdict.hole, None);
    assert!(verdict.skipped_low_curl.is_empty());
}

/// Issue #934: for curl-bearing pairs the new check reports exactly the
/// pair [`CheckedComplexEigenpairs::localized_hole`] reports (same
/// window, distance and nearest-`σ` rules), and a non-localized
/// spurious value in `rejected` never counts for either.
#[test]
fn analytic_cladding_hole_matches_localized_hole_for_curl_bearing_pairs() {
    let (window, sigma, _) = geometry_934();
    let floor = physical_curl_floor_pml();
    let checked = checked_934(
        &[
            (c64::new(35.70, -1e-9), 3e-6, 1e-3),
            (c64::new(35.85, 2e-9), 1e-5, 2e-4),
            (c64::new(35.20, 0.0), 1e-6, 5e-3),
        ],
        &[(c64::new(35.94, 0.0), 11.0)],
    );
    let in_window = |l: c64| l.re > window.0 && l.re < window.1;
    for reach in [0.01, 0.12, 0.2, 0.3, 0.5, 1.0] {
        let expected = checked.localized_hole(sigma, reach, in_window);
        let got =
            analytic_cladding_hole(&checked, sigma, reach, window, floor, curl_from_vector_934)
                .hole
                .map(|h| (h.beta_sq, h.residual));
        assert_eq!(got, expected, "reach {reach}");
    }
}

/// The hole is surfaced as the typed [`EigenError::SelectionHole`] naming
/// the withheld `β²`, its residual and the reference mode (issue #850).
#[test]
fn check_selection_hole_reports_typed_error() {
    let err = check_selection_hole(
        "solve_dielectric_modes2_pml",
        &[withheld_916(c64::new(35.5, 1e-12), 3e-6, PHYSICAL_CURL_916)],
        35.96,
        (34.0, 36.0),
        Some(35.0),
        &rule_913(physical_curl_floor_pml()),
        144,
    )
    .expect_err("hole must be an error");
    match &err {
        EigenError::SelectionHole {
            solver,
            beta_sq_re,
            residual,
            lowest_returned,
            lanczos_steps,
            ..
        } => {
            assert_eq!(*solver, "solve_dielectric_modes2_pml");
            assert_eq!(*beta_sq_re, 35.5);
            assert_eq!(*residual, 3e-6);
            assert_eq!(*lowest_returned, 35.0);
            assert_eq!(*lanczos_steps, 144);
        }
        other => panic!("expected SelectionHole, got {other:?}"),
    }
    let msg = err.to_string();
    assert!(
        msg.contains("3.550000e1") && msg.contains("Remedy"),
        "{msg}"
    );
}

fn selection_hole_err_850() -> EigenError {
    EigenError::SelectionHole {
        solver: "test",
        beta_sq_re: 35.5,
        beta_sq_im: 0.0,
        residual: 1e-6,
        residual_tol: DIELECTRIC_RESIDUAL_TOL,
        lowest_returned: 35.0,
        lanczos_steps: 40,
    }
}

/// Retry policy (issue #850): a hole retries once at a doubled request
/// and returns the retry's selection when that closes it; a hole that
/// survives the retry is a loud [`EigenError::SelectionHole`]; a clean
/// first attempt never retries.
#[test]
fn retry_on_selection_hole_policy() {
    let mut seen = Vec::new();
    let out = retry_on_selection_hole("t", 16, |n| {
        seen.push(n);
        Ok((vec![n], Ok(())))
    })
    .unwrap();
    assert_eq!((out, seen), (vec![16], vec![16]));

    let mut seen = Vec::new();
    let out = retry_on_selection_hole("t", 16, |n| {
        seen.push(n);
        let hole = if n == 16 {
            Err(selection_hole_err_850())
        } else {
            Ok(())
        };
        Ok((vec![n], hole))
    })
    .unwrap();
    assert_eq!((out, seen), (vec![32], vec![16, 32]));

    let mut seen = Vec::new();
    let err = retry_on_selection_hole("t", 40, |n| {
        seen.push(n);
        Ok((vec![n], Err(selection_hole_err_850())))
    })
    .unwrap_err();
    assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");
    assert_eq!(seen, vec![40, 80]);
}

/// Real p=2 family on a real pencil (issue #850): the ~3 %-step PEC fiber
/// of `formulation_audit_graddiv` at the classifier's own request (16)
/// converges one partner of the degenerate `β² = 35.385` pair and
/// withholds the other, so the 4-mode set would return `35.298` in its
/// place. The attempt flags the hole; with the budget held low on the
/// retry too it fails loudly; the public solve's doubled-request retry
/// closes it and returns both partners.
#[test]
fn real_p2_classifier_flags_degenerate_twin_hole() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
    let outer_r = 6.0 * a_um;
    let (mesh, tags) = disk_tri_mesh(a_um, outer_r, 5, 48);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);

    let (modes, hole) = solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 4, 16).unwrap();
    assert_eq!(modes.len(), 4);
    match hole {
        Err(EigenError::SelectionHole {
            beta_sq_re,
            lowest_returned,
            ..
        }) => {
            assert!((beta_sq_re - 35.385).abs() < 1e-2, "withheld {beta_sq_re}");
            assert!((lowest_returned - 35.298).abs() < 1e-2, "{lowest_returned}");
        }
        other => panic!("expected a selection hole at request 16, got {other:?}"),
    }

    // Issue #916: the withheld twin is a physical pair. Its Ritz vector
    // carries curl energy above the classifier's own floor (measured
    // 2.9e-3 against 6.0e-4), so the curl filter keeps it as a hole; it
    // also obeys the exact in-window bound r < (ε_max − ε_min)/ε_max.
    let (e_core, e_clad) = (n_core * n_core, n_clad * n_clad);
    let floor = physical_curl_floor_p2(e_core, e_clad);
    let raw = dielectric_raw_candidates_p2(
        &mesh,
        &eps,
        &interior,
        k0,
        16,
        physical_index_ceiling(&mesh, &eps, k0),
    )
    .unwrap();
    let twin = raw
        .localized_withheld
        .iter()
        .find(|c| (c.beta_sq.re - 35.385).abs() < 1e-2)
        .expect("the withheld twin is a localized withheld candidate");
    eprintln!(
        "withheld twin: β² = {:.6}, ρ = {:.2e}, curl = {:.3e}, floor = {floor:.3e}",
        twin.beta_sq.re, twin.residual, twin.curl_ratio
    );
    assert!(
        twin.curl_ratio > 2.0 * floor && twin.curl_ratio < (e_core - e_clad) / e_core,
        "twin curl ratio {:.3e} vs floor {floor:.3e}",
        twin.curl_ratio
    );

    // Budget forced low on both attempts: loud failure.
    let err = retry_on_selection_hole("solve_dielectric_modes2", 16, |_| {
        solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 4, 16)
    })
    .unwrap_err();
    assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");

    // Public solve: the retry closes the hole; both partners returned.
    let modes = solve_dielectric_modes2(&mesh, &eps, &interior, k0, 4).unwrap();
    let b: Vec<f64> = modes.iter().map(|m| m.beta_sq).collect();
    assert_eq!(b.len(), 4, "{b:?}");
    assert!(
        b.iter().filter(|x| (*x - 35.385).abs() < 1e-2).count() == 2,
        "both 35.385 partners after the retry: {b:?}"
    );
}

/// PML family on a real pencil (issue #850): the coarse `(3, 32)`
/// high-contrast PML fiber with `n_modes = 4` and the Lanczos request
/// forced down to 16 withholds the bound `β² ≈ 35.42` pair above the
/// lowest returned bound mode (`≈ 35.21`). The attempt flags it and,
/// with the budget held low on the retry, the solve fails loudly.
#[test]
fn pml_classifier_flags_hole_at_forced_low_request() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
    let clad_r = 8.0 * a_um;
    let outer_r = 11.0 * a_um;
    let (mesh, tags) = disk_tri_mesh_pml(a_um, clad_r, outer_r, 3, 32);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    let attempt = |n_request: usize| {
        solve_dielectric_modes2_pml_attempt(
            &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, n_request,
        )
    };
    let (_, hole) = attempt(16).unwrap();
    match hole {
        Err(EigenError::SelectionHole {
            beta_sq_re,
            beta_sq_im,
            lowest_returned,
            ..
        }) => {
            assert!((beta_sq_re - 35.42).abs() < 1e-2, "withheld {beta_sq_re}");
            assert!(beta_sq_im.abs() <= DIELECTRIC_BOUND_REL_IM * beta_sq_re);
            assert!(lowest_returned < beta_sq_re, "{lowest_returned}");
        }
        other => panic!("expected a selection hole at request 16, got {other:?}"),
    }
    let err =
        retry_on_selection_hole("solve_dielectric_modes2_pml", 16, |_| attempt(16)).unwrap_err();
    assert!(matches!(err, EigenError::SelectionHole { .. }), "{err:?}");
}

// ----- Empty-bound-set hole rule (issue #913) -----------------------

/// Issue #913: with no returned bound mode, a resolved, in-window,
/// bound-like withheld pair with physical curl is reported (verdict
/// `Hole`, which the empty-set rule turns into a warning), and each of
/// the empty-set conditions is necessary. On `main` every one of these
/// returned `None` (the rule could not fire without a reference).
#[test]
fn empty_bound_set_hole_requires_each_empty_set_condition() {
    let window = (34.0, 36.0);
    let sigma = 35.96;
    let k0 = 4.0;
    let (eps_max, eps_min) = (36.0 / (k0 * k0), 34.0 / (k0 * k0));
    // Classifier floor 1e-4, empty-set threshold 1e-3, Rayleigh test on.
    let rule = HoleRule {
        curl_floor: 1e-4,
        empty_set_curl_floor: 1e-3,
        rayleigh: Some((k0, eps_max, eps_min)),
    };
    // A pair obeying the Rayleigh identity r = 1 − β²/(k₀² ⟨ε⟩) exactly.
    let beta_sq = 35.5_f64;
    let consistent = |curl: f64| Some(beta_sq / (k0 * k0 * (1.0 - curl)));
    let pair = |im: f64, residual: f64, curl: f64, eps_weighted: Option<f64>| {
        [WithheldCandidate {
            beta_sq: c64::new(beta_sq, im),
            residual,
            curl_ratio: curl,
            eps_weighted,
        }]
    };
    let verdict = |p: &[WithheldCandidate; 1]| hole_verdict(&p[0], sigma, window, None, &rule);
    let hole = |p: &[WithheldCandidate; 1]| selection_hole(p, sigma, window, None, &rule);

    // Baseline: every condition met.
    let good = pair(0.0, 1e-6, 5e-3, consistent(5e-3));
    assert_eq!(verdict(&good), HoleVerdict::Hole);
    assert_eq!(hole(&good), Some(good[0]));
    // Exactly at the residual cap still counts; just above does not.
    assert_eq!(
        verdict(&pair(0.0, EMPTY_SET_RESIDUAL_CAP, 5e-3, consistent(5e-3))),
        HoleVerdict::Hole
    );
    let unresolved = pair(0.0, 1.01 * EMPTY_SET_RESIDUAL_CAP, 5e-3, consistent(5e-3));
    assert_eq!(verdict(&unresolved), HoleVerdict::Unresolved);
    assert_eq!(hole(&unresolved), None);
    // Above the classifier floor but at / below the empty-set threshold.
    for curl in [2e-4, 1e-3] {
        let low = pair(0.0, 1e-6, curl, consistent(curl));
        assert_eq!(verdict(&low), HoleVerdict::BelowEmptySetCurl, "{curl:e}");
        assert_eq!(hole(&low), None);
    }
    // At / below the classifier floor: the #916 label, as with a reference.
    assert_eq!(
        verdict(&pair(0.0, 1e-6, 1e-4, consistent(1e-4))),
        HoleVerdict::BelowCurlFloor
    );
    // Breaks the Rayleigh identity (⟨ε⟩ off by 1 %, δ ≈ 1e-2 against a
    // tolerance of 1e-3 · (ε_max − ε_min)/ε_max ≈ 5.6e-5).
    let off = pair(0.0, 1e-6, 5e-3, consistent(5e-3).map(|e| 1.01 * e));
    assert_eq!(verdict(&off), HoleVerdict::RayleighInconsistent);
    assert_eq!(hole(&off), None);
    // Curl at the exact in-window bound (ε_max − ε_min)/ε_max: no
    // in-window eigenpair reaches it.
    let bound = (eps_max - eps_min) / eps_max;
    assert_eq!(
        verdict(&pair(0.0, 1e-6, bound, consistent(bound))),
        HoleVerdict::RayleighInconsistent
    );
    // The PML path carries no ⟨ε⟩ and applies no Rayleigh test.
    assert_eq!(verdict(&pair(0.0, 1e-6, 5e-3, None)), HoleVerdict::Hole);
    // Leaky (|Im| / Re = 1e-6) or outside the certain window: never.
    assert_eq!(
        verdict(&pair(35.5e-6, 1e-6, 5e-3, None)),
        HoleVerdict::NotCandidate
    );
    let at_ceiling = [WithheldCandidate {
        beta_sq: c64::new(36.0 - 1e-14, 0.0),
        residual: 6e-8,
        curl_ratio: 5e-3,
        eps_weighted: None,
    }];
    assert_eq!(verdict(&at_ceiling), HoleVerdict::NotCandidate);
    // A NaN diagnostic is never a hole.
    assert_eq!(hole(&pair(0.0, f64::NAN, 5e-3, None)), None);
    assert_eq!(hole(&pair(0.0, 1e-6, f64::NAN, None)), None);

    // A window with no interior (uniform ε) admits no pair at all.
    let flat = (35.5, 35.5);
    assert_eq!(selection_hole(&good, sigma, flat, None, &rule), None);

    // The highest qualifying pair is the one reported.
    let two = [
        good[0],
        WithheldCandidate {
            beta_sq: c64::new(35.7, 0.0),
            residual: 1e-6,
            curl_ratio: 5e-3,
            eps_weighted: None,
        },
    ];
    assert_eq!(
        selection_hole(&two, sigma, window, None, &rule).map(|c| c.beta_sq.re),
        Some(35.7)
    );
}

/// Issue #913: the empty-set thresholds a classifier builds. The curl
/// threshold is twice the contrast-scaled floor on every path, the PML
/// path (own floor `1e-6`) included, and never below the classifier's
/// own floor.
#[test]
fn hole_rule_empty_set_threshold_is_contrast_scaled_on_every_path() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    // ~3 %-step fiber: contrast floor 6.0e-4 → threshold 1.2e-3.
    let (e_core, e_clad) = (1.4874_f64.powi(2), 1.4447_f64.powi(2));
    let contrast_floor = physical_curl_floor(e_core, e_clad);
    let pec = HoleRule::new(contrast_floor, e_core, e_clad, Some(k0));
    assert_eq!(pec.curl_floor, contrast_floor);
    assert_eq!(pec.empty_set_curl_floor, 2.0 * contrast_floor);
    assert_eq!(pec.rayleigh, Some((k0, e_core, e_clad)));
    let pml = HoleRule::new(physical_curl_floor_pml(), e_core, e_clad, None);
    assert_eq!(pml.curl_floor, 1e-6);
    assert_eq!(pml.empty_set_curl_floor, 2.0 * contrast_floor);
    assert!((pml.empty_set_curl_floor - 1.2e-3).abs() < 1e-5);
    assert_eq!(pml.rayleigh, None);
    // SMF-28: 1.58e-4, 4.5× below the measured LP01 curl ratio 7.2e-4.
    let smf = HoleRule::new(1e-6, 1.4504_f64.powi(2), e_clad, None);
    assert!((smf.empty_set_curl_floor - 1.58e-4).abs() < 1e-6);
    assert!(7.2e-4 > 4.0 * smf.empty_set_curl_floor);
    // Si/SiO2: capped floor 3e-2 → 6e-2, below the genuine band 8.5e-2.
    let si = HoleRule::new(3e-2, 3.45_f64.powi(2), 1.45_f64.powi(2), Some(k0));
    assert_eq!(si.empty_set_curl_floor, 6e-2);
    // Never below the classifier's own floor.
    assert_eq!(
        HoleRule::new(0.5, e_core, e_clad, None).empty_set_curl_floor,
        0.5
    );
}

/// Drain the empty-set rule evaluations recorded on this thread.
fn take_low_curl_events() -> Vec<LowCurlEvent> {
    LOW_CURL_EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn take_empty_set_events() -> Vec<EmptySetEvent> {
    EMPTY_SET_EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

/// Issue #913: the empty-set rule is warn-only. A reported pair is
/// recorded and logged with actionable text, and the check returns
/// `Ok`. No `SelectionHole` is built, so no `NaN` reference reaches a
/// public error value. With a reference the same pair is still the
/// typed #850 error.
#[test]
fn check_selection_hole_empty_set_warns_and_returns_ok() {
    let rule = rule_913(physical_curl_floor_pml());
    let window = (34.0, 36.0);
    let pair = [withheld_916(c64::new(35.5, 1e-12), 3e-6, PHYSICAL_CURL_916)];
    take_empty_set_events();
    let out = check_selection_hole(
        "solve_dielectric_modes2_pml",
        &pair,
        35.96,
        window,
        None,
        &rule,
        144,
    );
    assert!(out.is_ok(), "the empty-set rule never errors: {out:?}");
    assert_eq!(
        take_empty_set_events(),
        vec![("solve_dielectric_modes2_pml", Some(pair[0]))]
    );

    // The warning names the pair, says what it may be, and how to decide.
    let msg = empty_set_warning("solve_dielectric_modes2_pml", &pair[0], window, &rule, 144);
    for needle in [
        "solve_dielectric_modes2_pml: WARNING (issue #913, warn-only)",
        "no bound mode is returned",
        "β² = 3.550000e1",
        "window fraction b ≈ 0.750",
        "relative residual 3.000e-6",
        "after 144 Lanczos steps",
        "returned unchanged",
        "NOT confirmed as a guided mode",
        "issue #947",
        "radially refined mesh",
        "outer (PML or PEC box) radius",
        "request more modes",
    ] {
        assert!(msg.contains(needle), "missing {needle:?} in: {msg}");
    }
    assert!(!msg.contains("NaN"), "{msg}");

    // With a reference the rule is unchanged: the typed error, with a
    // finite reference, and no empty-set evaluation.
    let err = check_selection_hole(
        "solve_dielectric_modes2_pml",
        &pair,
        35.96,
        window,
        Some(35.0),
        &rule,
        144,
    )
    .unwrap_err();
    assert!(
        matches!(err, EigenError::SelectionHole { lowest_returned, .. } if lowest_returned == 35.0),
        "{err:?}"
    );
    let with_ref = err.to_string();
    assert!(
        with_ref.contains("above the lowest returned bound mode β² = 3.500000e1"),
        "{with_ref}"
    );
    assert!(take_empty_set_events().is_empty());

    // An unresolved pair (ρ above the cap) is evaluated and not reported.
    let unresolved = [withheld_916(c64::new(35.5, 1e-12), 3e-4, PHYSICAL_CURL_916)];
    assert!(check_selection_hole("t", &unresolved, 35.96, window, None, &rule, 144).is_ok());
    assert_eq!(take_empty_set_events(), vec![("t", None)]);
}

/// Issue #913: an empty-set report does not enter the retry policy.
/// The attempt runs once and its (empty) set is returned, whether or
/// not a pair was reported. A with-reference hole on the same pair
/// still retries and then errors.
#[test]
fn empty_bound_set_warning_never_retries_or_errors() {
    let rule = rule_913(physical_curl_floor_pml());
    let window = (34.0, 36.0);
    let withheld = [withheld_916(c64::new(35.5, 0.0), 3e-6, PHYSICAL_CURL_916)];
    let check = |pairs: &[WithheldCandidate], lowest: Option<f64>| {
        check_selection_hole("t", pairs, 35.96, window, lowest, &rule, 24)
    };

    // Nothing returned, a qualifying pair withheld: one attempt, empty
    // set, one report.
    take_empty_set_events();
    let mut seen = Vec::new();
    let out = retry_on_selection_hole("t", 16, |n| {
        seen.push(n);
        Ok((Vec::<f64>::new(), check(&withheld, None)))
    })
    .unwrap();
    assert_eq!((out.len(), seen), (0, vec![16]));
    assert_eq!(take_empty_set_events(), vec![("t", Some(withheld[0]))]);

    // Only a gradient pair withheld: the same, with nothing reported.
    let gradient = [withheld_916(c64::new(35.5, 0.0), 3e-6, 1e-13)];
    let mut seen = Vec::new();
    let out = retry_on_selection_hole("t", 16, |n| {
        seen.push(n);
        Ok((Vec::<f64>::new(), check(&gradient, None)))
    })
    .unwrap();
    assert_eq!((out.len(), seen), (0, vec![16]));
    assert_eq!(take_empty_set_events(), vec![("t", None)]);

    // The same pair above a returned mode: retry, then the #850 error.
    let mut seen = Vec::new();
    let err = retry_on_selection_hole("t", 16, |n| {
        seen.push(n);
        Ok((vec![35.0_f64], check(&withheld, Some(35.0))))
    })
    .unwrap_err();
    assert_eq!(seen, vec![16, 32]);
    assert!(
        matches!(err, EigenError::SelectionHole { lowest_returned, .. } if lowest_returned == 35.0),
        "{err:?}"
    );
    assert!(take_empty_set_events().is_empty());
}

/// The ~3 %-step PEC fiber of `formulation_audit_graddiv` (mesh
/// `(5, 48)`), shared by the real-path #913 tests.
fn graddiv_fiber_913() -> (TriMesh, Vec<f64>, Vec<bool>, f64, (f64, f64)) {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n_core, n_clad, a_um) = (1.4874_f64, 1.4447_f64, 1.40_f64);
    let outer_r = 6.0 * a_um;
    let (mesh, tags) = disk_tri_mesh(a_um, outer_r, 5, 48);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    (mesh, eps, interior, k0, (n_core * n_core, n_clad * n_clad))
}

/// Issue #913 on a real pencil, real path: the localized withheld pairs
/// of the request-16 graddiv solve are a low-curl ladder, not a
/// gradient cluster. Evaluated with **no** reference mode (by hand:
/// this solve does return bound modes, so its classifier never applies
/// the empty-set rule), the rule
/// reports the genuine `β² = 35.385` twin (curl `4.8×` the floor) and,
/// once that is set aside, none of the ladder pairs: the `β² = 35.124`
/// pair clears the classifier's floor by 1 % (so a plain
/// "above the floor" empty-set rule would report it) and is excluded by
/// the margin; the rest are at or below the floor.
#[test]
fn real_p2_low_curl_ladder_is_not_an_empty_set_hole() {
    let (mesh, eps, interior, k0, (e_core, e_clad)) = graddiv_fiber_913();
    let floor = physical_curl_floor_p2(e_core, e_clad);
    let rule = HoleRule::new(floor, e_core, e_clad, Some(k0));
    let ceiling = physical_index_ceiling(&mesh, &eps, k0);
    let raw = dielectric_raw_candidates_p2(&mesh, &eps, &interior, k0, 16, ceiling).unwrap();
    let n_ceiling = ceiling.unwrap_or(e_core.sqrt());
    let window = (e_clad * k0 * k0, n_ceiling * n_ceiling * k0 * k0);
    for c in &raw.localized_withheld {
        eprintln!(
            "withheld β² = {:.5}, ρ = {:.2e}, curl = {:.3e} ({:.3}× floor): {:?}",
            c.beta_sq.re,
            c.residual,
            c.curl_ratio,
            c.curl_ratio / floor,
            hole_verdict(c, raw.sigma, window, None, &rule)
        );
    }

    // The twin is the pair the empty-set rule reports.
    let hit = selection_hole(&raw.localized_withheld, raw.sigma, window, None, &rule)
        .expect("the withheld twin is reported with no reference mode");
    assert!((hit.beta_sq.re - 35.385).abs() < 1e-2, "{hit:?}");
    assert!(hit.residual <= EMPTY_SET_RESIDUAL_CAP && hit.curl_ratio > 4.0 * floor);

    // The ladder below it: no hole.
    let ladder: Vec<WithheldCandidate> = raw
        .localized_withheld
        .iter()
        .copied()
        .filter(|c| (c.beta_sq.re - 35.385).abs() >= 1e-2)
        .collect();
    assert!(ladder.len() >= 4, "{ladder:?}");
    assert_eq!(
        selection_hole(&ladder, raw.sigma, window, None, &rule),
        None
    );
    // The near-floor pair: resolved, in-window, above the classifier's
    // floor by about 1 %, and excluded only by the curl margin.
    let near = ladder
        .iter()
        .find(|c| (c.beta_sq.re - 35.124).abs() < 1e-2)
        .expect("the β² = 35.124 pair is withheld at request 16");
    assert!(
        near.curl_ratio > floor && near.curl_ratio < 1.1 * floor,
        "curl {:.4e} vs floor {floor:.4e}",
        near.curl_ratio
    );
    assert!(near.residual <= EMPTY_SET_RESIDUAL_CAP, "{near:?}");
    assert_eq!(
        hole_verdict(near, raw.sigma, window, None, &rule),
        HoleVerdict::BelowEmptySetCurl
    );
    // Every other ladder pair is at / below the classifier's floor.
    for c in ladder.iter().filter(|c| c.beta_sq.re != near.beta_sq.re) {
        assert_eq!(
            hole_verdict(c, raw.sigma, window, None, &rule),
            HoleVerdict::BelowCurlFloor,
            "{c:?}"
        );
    }
}

/// Issue #913: `n_modes = 0` asks for nothing, so the empty result is
/// not an empty-set hole: the rule is not evaluated and nothing is
/// warned, even on the graddiv solve whose request-16 pass withholds
/// the genuine twin.
#[test]
fn zero_requested_modes_is_never_an_empty_set_hole() {
    let (mesh, eps, interior, k0, _) = graddiv_fiber_913();
    take_empty_set_events();
    let (modes, hole) = solve_dielectric_modes2_attempt(&mesh, &eps, &interior, k0, 0, 16).unwrap();
    assert!(modes.is_empty());
    assert!(hole.is_ok(), "{hole:?}");
    assert_eq!(take_empty_set_events(), Vec::new());
}

/// A PML disk fixture `(mesh, tags, eps, interior, clad_r, outer_r)`
/// for the #913 PML tests.
fn pml_disk_913(
    (n_core, n_clad): (f64, f64),
    core_r: f64,
    (clad_r, outer_r): (f64, f64),
    (n_rings, n_theta): (usize, usize),
) -> (TriMesh, Vec<i32>, Vec<f64>, Vec<bool>) {
    let (mesh, tags) = disk_tri_mesh_pml(core_r, clad_r, outer_r, n_rings, n_theta);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    (mesh, tags, eps, interior)
}

/// The thin-core fiber of the #913 PML tests: `a = 0.30 µm`,
/// `n = 1.4874 / 1.4447`, PML box `(8, 11) µm`. `V ≈ 0.43`.
const THIN_CORE_913: ((f64, f64), f64, (f64, f64)) = ((1.4874, 1.4447), 0.30, (8.0, 11.0));

/// Issue #913 end to end on a real pencil, PML path, and the reason
/// the empty-set rule only warns.
///
/// On the thin-core fiber ([`THIN_CORE_913`], mesh `(4, 48)`) at a
/// Lanczos request of 6 the classifier returns **no** bound mode while
/// its solve withholds a resolved pair at `β² ≈ 35.155`
/// (`ρ ≈ 8×10⁻⁸`). The empty-set rule reports that pair.
///
/// The pair is **not** a withheld guided mode. This fiber has
/// `V ≈ 0.43`, and its only guided mode, LP₀₁, has an analytic
/// `b = 4.7×10⁻⁹` (checked below against [`fiber_lp_neff`]). The
/// reported pair has `b ≈ 0.42`. The Judge's review of PR #948 measured
/// that it drifts under radial refinement (`b = 0.417, 0.468, 0.529`)
/// and with the PML box radius, and tracks its core energy fraction: it
/// is a member of the low-curl ladder of issue #947, which the PML
/// classifier returns as a "bound mode" once it converges (request 12
/// below). The empty set at request 6 is the correct result.
///
/// So the test pins warn-only: the pair is reported, the attempt and
/// the retry policy return the empty set after one solve, and no error
/// is raised. An error-after-retry policy would have replaced that
/// empty set by the artifact.
///
/// [`fiber_lp_neff`]: crate::analytic::fiber::fiber_lp_neff
#[test]
fn pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01() {
    use crate::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let ((n_core, n_clad), a, (clad_r, outer_r)) = THIN_CORE_913;

    // Analytic: V ≈ 0.43, LP01 (the only guided mode) at b ≈ 4.7e-9.
    let v = v_number(n_core, n_clad, a, k0);
    assert!((v - 0.4303).abs() < 1e-3, "V = {v}");
    let n_lp01 = fiber_lp_neff(n_core, n_clad, a, k0, 0, 1).expect("LP01 exists for all V");
    let b_lp01 = normalized_b(n_lp01, n_core, n_clad);
    assert!(
        b_lp01 > 0.0 && b_lp01 < 1e-7,
        "analytic LP01 b = {b_lp01:e}"
    );
    assert!(fiber_lp_neff(n_core, n_clad, a, k0, 0, 2).is_none());
    assert!(fiber_lp_neff(n_core, n_clad, a, k0, 1, 1).is_none());

    let (mesh, tags, eps, interior) = pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), (4, 48));
    let window = (n_clad * n_clad * k0 * k0, n_core * n_core * k0 * k0);
    let attempt = |n_request: usize| {
        solve_dielectric_modes2_pml_attempt(
            &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, n_request,
        )
    };
    let bound = |modes: &[DielectricModePml]| -> Vec<f64> {
        modes
            .iter()
            .filter(|m| m.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs())
            .map(|m| m.beta_sq.re)
            .collect()
    };

    // Request 6: no bound mode, no error, and one empty-set report.
    take_empty_set_events();
    let (modes, hole) = attempt(6).unwrap();
    assert!(bound(&modes).is_empty(), "request 6 returns no bound mode");
    assert!(hole.is_ok(), "the empty-set rule is warn-only: {hole:?}");
    let events = take_empty_set_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, "solve_dielectric_modes2_pml");
    let pair = events[0]
        .1
        .expect("the request-6 solve reports a withheld pair");
    assert!((pair.beta_sq.re - 35.155).abs() < 2e-2, "{pair:?}");
    assert!(pair.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * pair.beta_sq.re);
    assert!(pair.residual <= EMPTY_SET_RESIDUAL_CAP, "{pair:?}");

    // The reported pair is nowhere near the analytic LP01: b ≈ 0.42
    // against 4.7e-9. It clears the curl threshold by about 1.33×.
    let b_pair = (pair.beta_sq.re - window.0) / (window.1 - window.0);
    assert!((b_pair - 0.417).abs() < 2e-2, "b = {b_pair}");
    assert!(b_pair > 1e6 * b_lp01, "b = {b_pair} vs LP01 {b_lp01:e}");
    let rule = HoleRule::new(
        physical_curl_floor_pml(),
        n_core * n_core,
        n_clad * n_clad,
        None,
    );
    let over = pair.curl_ratio / rule.empty_set_curl_floor;
    assert!(over > 1.0 && over < 1.7, "curl {over:.3}× the threshold");
    let msg = empty_set_warning("solve_dielectric_modes2_pml", &pair, window, &rule, 0);
    assert!(
        msg.contains("window fraction b ≈ 0.41") && msg.contains("issue #947"),
        "{msg}"
    );

    // The retry policy does not act on it: one solve, the empty set.
    let mut seen = Vec::new();
    let modes = retry_on_selection_hole("solve_dielectric_modes2_pml", 6, |n| {
        seen.push(n);
        attempt(n)
    })
    .unwrap();
    assert_eq!(seen, vec![6]);
    assert!(bound(&modes).is_empty());
    take_empty_set_events();

    // Issue #947, not this rule: at request 12 the same pair converges
    // and the PML classifier returns it as its top "bound mode". Its
    // curl ratio (≈ 2.6× the contrast-scaled classifier floor) clears
    // that floor, so it is returned, but it carries the low-curl flag.
    take_low_curl_events();
    let (modes, hole) = attempt(12).unwrap();
    assert!(hole.is_ok(), "{hole:?}");
    let b = bound(&modes);
    assert!(
        (b[0] - pair.beta_sq.re).abs() < 1e-6,
        "the ladder pair {} is the top returned bound mode at request 12: {b:?}",
        pair.beta_sq.re
    );
    assert!(take_empty_set_events().is_empty());
    let flags = take_low_curl_events();
    assert!(
        b.iter()
            .all(|re| flags.iter().any(|f| (f.1 - re).abs() < 1e-9)),
        "every returned ladder mode is flagged: {b:?} vs {flags:?}"
    );
    let over = flags[0].2 / pml_classifier_curl_floor(n_core * n_core, n_clad * n_clad);
    assert!(over > 1.0 && over < PML_LOW_CURL_WARN_MARGIN, "{over}");

    // The profile selector on the request-6 solve scores no bound
    // candidate. It reports the same pair and returns its empty ladder.
    let template = Lp01RadialTemplate::from_oracle_b(a, 0.43, 0.5);
    let (scored, hole) = solve_dielectric_modes2_pml_profile_selected_attempt(
        &mesh,
        &eps,
        &tags,
        &interior,
        clad_r,
        outer_r,
        6.0,
        k0,
        &template,
        (24, 4.0, 0.2),
        6,
    )
    .unwrap();
    assert!(scored.is_empty());
    assert!(hole.is_ok(), "{hole:?}");
    let events = take_empty_set_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, "solve_dielectric_modes2_pml_profile_selected");
    let selector_pair = events[0].1.expect("the selector reports the same pair");
    assert!((selector_pair.beta_sq.re - pair.beta_sq.re).abs() < 1e-6);
}

/// Issue #913 on a real pencil that guides nothing, PML path: a weak
/// **anti-guide** (`n_core = 1.4447 < n_clad = 1.4504`, SMF-28 contrast
/// reversed) on mesh `(5, 48)` at a Lanczos request of 16. The
/// classifier returns no bound mode and its solve withholds a ladder of
/// **resolved**, in-window, bound-like PML-box pairs whose curl ratio
/// (`≈ 6×10⁻⁶`) is several times the PML classifier's own floor. A
/// rule that used that floor would report them. The contrast-scaled
/// threshold (`1.58×10⁻⁴`) does not: the attempt evaluates the
/// empty-set rule and reports nothing.
///
/// At its production request of 40 the solve converges those ladder
/// pairs. Under the old `10⁻⁶` classifier floor the public call
/// returned four of them as bound modes of the anti-guide (issue #947).
/// The contrast-scaled classifier floor ([`pml_classifier_curl_floor`],
/// `7.9×10⁻⁵` here; the ladder sits at `0.05…0.12×` it) drops them: the
/// public call returns no bound mode and flags nothing.
#[test]
fn pml_antiguide_low_curl_ladder_is_not_an_empty_set_hole() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n_core, n_clad, a) = (1.4447_f64, 1.4504_f64, 4.1_f64);
    let (clad_r, outer_r) = (8.0 * a, 11.0 * a);
    let (mesh, tags, eps, interior) = pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), (5, 48));
    let (e_max, e_min) = (n_clad * n_clad, n_core * n_core);
    let window = (e_min * k0 * k0, e_max * k0 * k0);

    // The withheld pairs of the request-16 solve.
    let raw = dielectric_raw_candidates_p2_pml(
        &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 16, None,
    )
    .unwrap();
    let pml_floor = physical_curl_floor_pml();
    let rule = HoleRule::new(pml_floor, e_max, e_min, None);
    assert!((rule.empty_set_curl_floor - 1.58e-4).abs() < 1e-6);
    // Pairs that pass every test but the empty-set curl threshold.
    let ladder: Vec<&WithheldCandidate> = raw
        .localized_withheld
        .iter()
        .filter(|c| {
            hole_verdict(c, raw.sigma, window, None, &rule) == HoleVerdict::BelowEmptySetCurl
        })
        .collect();
    assert!(
        ladder.len() >= 3,
        "expected a resolved low-curl bound-like ladder, got {} pair(s)",
        ladder.len()
    );
    for c in &ladder {
        assert!(c.residual <= EMPTY_SET_RESIDUAL_CAP, "{c:?}");
        assert!(
            c.curl_ratio > 2.0 * pml_floor && c.curl_ratio < 0.2 * rule.empty_set_curl_floor,
            "curl {:.3e}",
            c.curl_ratio
        );
    }
    assert_eq!(
        selection_hole(&raw.localized_withheld, raw.sigma, window, None, &rule),
        None
    );
    // The same pairs under the classifier's own floor: a hole.
    let naive = HoleRule {
        empty_set_curl_floor: pml_floor,
        ..rule
    };
    assert!(
        selection_hole(&raw.localized_withheld, raw.sigma, window, None, &naive).is_some(),
        "the PML floor alone would report the ladder"
    );

    // The classifier attempt at that request: no bound mode, and the
    // empty-set rule is evaluated once and reports nothing.
    take_empty_set_events();
    let (modes, hole) = solve_dielectric_modes2_pml_attempt(
        &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, 16,
    )
    .unwrap();
    assert!(
        modes
            .iter()
            .all(|m| m.beta_sq.im.abs() > DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs()),
        "request 16 returns no bound mode"
    );
    assert!(hole.is_ok(), "{hole:?}");
    assert_eq!(
        take_empty_set_events(),
        vec![("solve_dielectric_modes2_pml", None)]
    );

    // Issue #947: the public call at its production request converges
    // the ladder and returns none of it as a bound mode.
    assert!(pml_classifier_curl_floor(e_max, e_min) > 5.0 * ladder[0].curl_ratio);
    take_low_curl_events();
    let modes =
        solve_dielectric_modes2_pml(&mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4)
            .expect("the anti-guide solve returns, it does not error");
    let bound: Vec<f64> = modes
        .iter()
        .filter(|m| m.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs())
        .map(|m| (m.beta_sq.re - window.0) / (window.1 - window.0))
        .collect();
    assert!(
        bound.is_empty(),
        "the anti-guide guides nothing, but the PML classifier returned bound mode(s) at \
         window fraction b = {bound:?} (issue #947)"
    );
    assert_eq!(take_low_curl_events(), Vec::new());
    take_empty_set_events();
}

/// Issue #913 acceptance: a cross-section with **no** guided mode still
/// returns an empty `Vec`, no error and no warning from every
/// classifier. A uniform `ε` has an empty guided window, so no withheld
/// pair can lie inside it (the request-16 solves below do withhold
/// localized pairs).
#[test]
fn mode_free_uniform_cross_section_returns_empty_without_error() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let (n, a) = (1.4447_f64, 1.40_f64);

    let outer_r = 6.0 * a;
    let (mesh, tags) = disk_tri_mesh(a, outer_r, 5, 48);
    let eps = vec![n * n; tags.len()];
    let (_, interior1) = disk_pec_interior_edges(&mesh, outer_r);
    let interior2 = disk_pec_interior_dofs2(&mesh, outer_r);
    take_empty_set_events();
    assert!(
        solve_dielectric_modes(&mesh, &eps, &interior1, k0, 4)
            .unwrap()
            .is_empty()
    );
    assert!(
        solve_dielectric_modes2(&mesh, &eps, &interior2, k0, 4)
            .unwrap()
            .is_empty()
    );

    let (clad_r, outer_r) = (8.0 * a, 11.0 * a);
    let (mesh, tags, eps, interior) = pml_disk_913((n, n), a, (clad_r, outer_r), (3, 32));
    assert!(
        solve_dielectric_modes2_pml(&mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4)
            .unwrap()
            .is_empty()
    );
    // Each classifier evaluated the empty-set rule once: nothing reported.
    let events = take_empty_set_events();
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(events.iter().all(|(_, pair)| pair.is_none()), "{events:?}");
}

/// One row of [`empty_set_rule_stress_sweep_913`]: classifier solves,
/// those that returned no bound mode (empty-set rule evaluations),
/// reports, and with-reference holes.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct SweepRow913 {
    solves: usize,
    evaluations: usize,
    reports: usize,
    reference_holes: usize,
}

/// Issue #913: a reduced, reproducible version of the stress sweep
/// behind the empty-set thresholds, with the rule's reports recorded.
///
/// Four fibers on two meshes, `n_modes = 4`:
///
/// - two **anti-guides**, which guide nothing: SMF-28 contrast
///   reversed (`1.4447 / 1.4504`, `a = 4.1 µm`) and ~3 % contrast
///   reversed (`1.4447 / 1.4874`, `a = 1.4 µm`);
/// - a **uniform** disk (`n = 1.4447`), whose guided window is empty;
/// - the **thin core** [`THIN_CORE_913`], whose only guided mode sits at
///   an analytic `b = 4.7×10⁻⁹`, so that the empty set is the correct
///   result at any resolvable precision.
///
/// Each runs the PEC p=1 and p=2 classifier attempts at Lanczos
/// requests 4, 8 and 16 (16 is their own) and the PML attempt at
/// requests 3 to 8, 12, 16 and 40 (40 is its own). 120 solves.
///
/// What it asserts: the rule reports nothing on the anti-guides, the
/// uniform disk or any PEC solve. Every report is on the thin-core PML
/// solves and is the low-curl ladder pair of issue #947 (`b ≈ 0.42` on
/// mesh `(4, 48)`), a known artifact and not a guided mode. That is why
/// the rule only warns. No attempt returns an error from the empty-set
/// branch. The per-row counts are printed (`--nocapture`).
///
/// Measured (release, macOS aarch64; 14.6, 49, 54 and 63 s over four
/// runs on a heavily loaded host): 120 solves, 87 empty-set
/// evaluations, 5 reports. By fiber, evaluations / reports: anti-guides
/// 34 / 0 (all PML; their PEC solves return box modes of the
/// higher-index cladding, so the rule is not evaluated), uniform
/// 30 / 0, thin core PEC 12 / 0, thin core PML 11 / 5. The 5 reports
/// are mesh `(4, 48)` requests 6 and 7 (`b = 0.417`) and mesh `(6, 64)`
/// requests 3, 5 and 6 (`b = 0.468`). The assertions pin the claims
/// above, not these counts, which can shift with Lanczos round-off.
///
/// Before issue #947 the same sweep saw the with-reference #850 rule
/// fire twice on the thin-core PML solves of mesh `(6, 64)` (the
/// withheld ladder pair `β² = 35.259` above a returned lower ladder
/// pair). With the contrast-scaled PML classifier floor of #947 those
/// lower ladder pairs are no longer returned, and a rerun (release,
/// 58 s) counts 0 with-reference holes, 90 evaluations and 6 reports,
/// all of them the same ladder pair (`b = 0.417` on `(4, 48)`, `0.468`
/// on `(6, 64)`).
#[test]
#[ignore = "release-tier stress sweep: 120 classifier solves, 15 to 63 s in release over four runs (local, Apple M3 Ultra, host load average above 100), not timed in debug; run with `cargo test -p geode-core --release --lib -- --ignored --exact analytic::waveguide::tests::empty_set_rule_stress_sweep_913 --nocapture`"]
fn empty_set_rule_stress_sweep_913() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    const PEC_REQUESTS: [usize; 3] = [4, 8, 16];
    const PML_REQUESTS: [usize; 9] = [3, 4, 5, 6, 7, 8, 12, 16, 40];
    const MESHES: [(usize, usize); 2] = [(4, 48), (6, 64)];
    // (name, (n_core, n_clad), core radius, PEC outer radius, PML box).
    type Fiber = (&'static str, (f64, f64), f64, f64, (f64, f64));
    let (thin_n, thin_a, thin_box) = THIN_CORE_913;
    let fibers: [Fiber; 4] = [
        (
            "anti-guide SMF-28",
            (1.4447, 1.4504),
            4.1,
            24.6,
            (32.8, 45.1),
        ),
        ("anti-guide ~3 %", (1.4447, 1.4874), 1.4, 8.4, (11.2, 15.4)),
        ("uniform", (1.4447, 1.4447), 1.4, 8.4, (11.2, 15.4)),
        ("thin core", thin_n, thin_a, 8.0, thin_box),
    ];

    // Tally one attempt: its hole-check outcome and the empty-set
    // events it recorded. Returns the reported pairs.
    let tally = |row: &mut SweepRow913, hole: Result<(), EigenError>| {
        row.solves += 1;
        match hole {
            Ok(()) => {}
            Err(EigenError::SelectionHole {
                beta_sq_re,
                lowest_returned,
                ..
            }) => {
                // Only the with-reference rule (#850) can error.
                assert!(lowest_returned.is_finite(), "NaN reference in an error");
                assert!(beta_sq_re > lowest_returned);
                eprintln!(
                    "WITH-REFERENCE HOLE (#850 rule): withheld β² = {beta_sq_re:.5} above \
                     returned {lowest_returned:.5}"
                );
                row.reference_holes += 1;
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
        let events = take_empty_set_events();
        assert!(events.len() <= 1, "{events:?}");
        let mut reported = Vec::new();
        for (_, pair) in events {
            row.evaluations += 1;
            if let Some(pair) = pair {
                row.reports += 1;
                reported.push(pair);
            }
        }
        reported
    };

    take_empty_set_events();
    let mut rows: Vec<(String, SweepRow913)> = Vec::new();
    // (mesh, request, b) of every PML report.
    let mut pml_reports: Vec<((usize, usize), usize, f64)> = Vec::new();
    for (name, (n_core, n_clad), a, pec_outer, (clad_r, outer_r)) in fibers {
        let eps_of = |tags: &[i32]| {
            epsilon_r_from_region_tags(tags, |t| {
                if t == REGION_CORE {
                    n_core * n_core
                } else {
                    n_clad * n_clad
                }
            })
        };
        let (mut p1, mut p2, mut pml) = <(SweepRow913, SweepRow913, SweepRow913)>::default();
        for mesh_size in MESHES {
            let (mesh, tags) = disk_tri_mesh(a, pec_outer, mesh_size.0, mesh_size.1);
            let eps = eps_of(&tags);
            let (_, interior1) = disk_pec_interior_edges(&mesh, pec_outer);
            let interior2 = disk_pec_interior_dofs2(&mesh, pec_outer);
            for request in PEC_REQUESTS {
                let (_, hole) =
                    solve_dielectric_modes_attempt(&mesh, &eps, &interior1, k0, 4, request)
                        .unwrap();
                tally(&mut p1, hole);
                let (_, hole) =
                    solve_dielectric_modes2_attempt(&mesh, &eps, &interior2, k0, 4, request)
                        .unwrap();
                tally(&mut p2, hole);
            }

            let (mesh, tags, eps, interior) =
                pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), mesh_size);
            let (e_max, e_min) = (
                (n_core * n_core).max(n_clad * n_clad),
                (n_core * n_core).min(n_clad * n_clad),
            );
            for request in PML_REQUESTS {
                let (_, hole) = solve_dielectric_modes2_pml_attempt(
                    &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4, request,
                )
                .unwrap();
                for pair in tally(&mut pml, hole) {
                    let b = (pair.beta_sq.re - e_min * k0 * k0) / ((e_max - e_min) * k0 * k0);
                    eprintln!(
                        "REPORT {name} PML mesh {mesh_size:?} request {request}: β² = {:.5}, \
                         b = {b:.4}, ρ = {:.2e}, curl = {:.3e}",
                        pair.beta_sq.re, pair.residual, pair.curl_ratio
                    );
                    assert_eq!(name, "thin core", "a report on a fiber that guides nothing");
                    pml_reports.push((mesh_size, request, b));
                }
            }
        }
        rows.push((format!("{name}, PEC p=1"), p1));
        rows.push((format!("{name}, PEC p=2"), p2));
        rows.push((format!("{name}, PML"), pml));
    }

    eprintln!(
        "\n#913 reduced stress sweep: solves / empty-set evaluations / reports / with-reference holes"
    );
    let mut total = SweepRow913::default();
    for (name, row) in &rows {
        eprintln!(
            "  {name:<28} {:>3} / {:>3} / {:>2} / {:>2}",
            row.solves, row.evaluations, row.reports, row.reference_holes
        );
        total.solves += row.solves;
        total.evaluations += row.evaluations;
        total.reports += row.reports;
        total.reference_holes += row.reference_holes;
    }
    eprintln!(
        "  {:<28} {:>3} / {:>3} / {:>2} / {:>2}",
        "total", total.solves, total.evaluations, total.reports, total.reference_holes
    );
    eprintln!("  thin-core PML reports (mesh, request, b): {pml_reports:?}");

    let row = |name: &str| rows.iter().find(|(n, _)| n == name).unwrap().1;
    // Fibers that guide nothing: evaluated, never reported.
    for fiber in ["anti-guide SMF-28", "anti-guide ~3 %", "uniform"] {
        for path in ["PEC p=1", "PEC p=2", "PML"] {
            let r = row(&format!("{fiber}, {path}"));
            assert_eq!(r.reports, 0, "{fiber}, {path}: {r:?}");
        }
        assert!(row(&format!("{fiber}, PML")).evaluations >= 12, "{fiber}");
    }
    // A uniform disk has no bound mode on any path: all 30 solves are
    // empty-set evaluations.
    for path in ["PEC p=1", "PEC p=2", "PML"] {
        let r = row(&format!("uniform, {path}"));
        assert_eq!(r.evaluations, r.solves, "uniform, {path}: {r:?}");
    }
    // Thin core, PEC: no bound mode (correct), nothing reported.
    for path in ["PEC p=1", "PEC p=2"] {
        let r = row(&format!("thin core, {path}"));
        assert_eq!((r.evaluations, r.reports), (r.solves, 0), "{path}: {r:?}");
    }
    // Thin core, PML: the only reports of the sweep, all of them the
    // #947 ladder pair (b ≈ 0.42 on mesh (4, 48), 0.47 on (6, 64);
    // the analytic LP01 is at b = 4.7e-9), including the request-6 case
    // of `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01`.
    assert_eq!(total.reports, row("thin core, PML").reports);
    assert!(
        pml_reports
            .iter()
            .any(|&(mesh_size, request, _)| mesh_size == (4, 48) && request == 6),
        "{pml_reports:?}"
    );
    for &(mesh_size, request, b) in &pml_reports {
        let expected = if mesh_size == (4, 48) { 0.417 } else { 0.468 };
        assert!(
            (b - expected).abs() < 2e-2,
            "mesh {mesh_size:?} request {request}: b = {b}"
        );
    }
    // Never at the PML classifier's own request.
    assert!(pml_reports.iter().all(|&(_, request, _)| request != 40));
}

/// One PML fixture of the issue #947 measurement:
/// `(name, (n_core, n_clad), core radius, (clad_r, outer_r), meshes)`.
type Fixture947 = (
    &'static str,
    (f64, f64),
    f64,
    (f64, f64),
    &'static [(usize, usize)],
);

/// The fixtures of the issue #947 curl-floor measurement: two
/// anti-guides and the thin core (all guide nothing resolvable), the
/// SMF-28 and high-contrast PML benchmarks (`step_index_fiber_benchmark`,
/// `high_contrast_fiber_benchmark`, both at their tier-2 meshes).
fn fixtures_947() -> [Fixture947; 5] {
    const BENCH: &[(usize, usize)] = &[(4, 48), (5, 60), (6, 72), (8, 96), (10, 120)];
    let (thin_n, thin_a, thin_box) = THIN_CORE_913;
    [
        (
            "anti-guide SMF-28",
            (1.4447, 1.4504),
            4.1,
            (8.0 * 4.1, 11.0 * 4.1),
            &[(5, 48), (6, 64)],
        ),
        (
            "anti-guide ~3 %",
            (1.4447, 1.4874),
            1.4,
            (8.0 * 1.4, 11.0 * 1.4),
            &[(5, 48), (6, 64)],
        ),
        (
            "thin core",
            thin_n,
            thin_a,
            thin_box,
            &[(4, 48), (6, 64), (8, 96)],
        ),
        (
            "SMF-28 bench",
            (1.4504, 1.4447),
            4.1,
            (8.0 * 4.1, 11.0 * 4.1),
            BENCH,
        ),
        (
            "high-contrast bench",
            (1.4874, 1.4447),
            1.4,
            (8.0 * 1.4, 11.0 * 1.4),
            BENCH,
        ),
    ]
}

/// Issue #947 measurement: the curl ratio of every in-window, bound
/// PML pair, as a multiple of the contrast-scaled floor
/// [`physical_curl_floor`]`(ε_max, ε_min)`, and the public
/// [`solve_dielectric_modes2_pml`] outcome under the classifier floor
/// [`pml_classifier_curl_floor`].
///
/// For each fixture and mesh it prints
///
/// - every converged in-window bound pair of the request-40 solve:
///   `β²`, window fraction `b`, curl ratio, `× floor` (contrast-scaled),
///   core-energy fraction, and whether the old `10⁻⁶` floor and the
///   contrast-scaled floor keep it;
/// - every localized, in-window, bound-like withheld pair of that solve
///   (`ρ`, curl ratio and `× floor`);
/// - the public call's outcome: the returned bound set (`b`, `× floor`)
///   or the error.
///
/// The decision rule of issue #947 reads off this table. What the
/// assertions pin, on the public call: every benchmark returns a bound
/// fundamental at least 3× the contrast-scaled floor (measured
/// `10.08 … 10.55×`) and raises no low-curl flag; both anti-guides
/// return no bound mode; and the thin core either errors loudly
/// (`SelectionHole`) or returns only bound modes that carry the
/// low-curl flag of [`warn_low_curl_pml_mode`] (its ladder pair sits at
/// `2.64 … 2.72×` the floor, above `1×`, so it is flagged, not
/// dropped; see [`pml_classifier_curl_floor`]).
#[test]
#[ignore = "release-tier measurement: about 40 PML solves; run with `cargo test -p geode-core --release --lib -- --ignored --exact analytic::waveguide::tests::pml_curl_floor_measurement_947 --nocapture`"]
fn pml_curl_floor_measurement_947() {
    let k0 = 2.0 * std::f64::consts::PI / 1.55;
    let k0_sq = k0 * k0;
    // (fixture, mesh, selected fundamental's × floor) of each benchmark.
    let mut fundamentals: Vec<(&str, (usize, usize), f64)> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (name, (n_core, n_clad), a, (clad_r, outer_r), meshes) in fixtures_947() {
        let (e_max, e_min) = (
            (n_core * n_core).max(n_clad * n_clad),
            (n_core * n_core).min(n_clad * n_clad),
        );
        let floor = physical_curl_floor(e_max, e_min);
        let window = (e_min * k0_sq, e_max * k0_sq);
        let b_of = |re: f64| (re - window.0) / (window.1 - window.0);
        let bound_c = |l: c64| l.im.abs() <= DIELECTRIC_BOUND_REL_IM * l.re.abs();
        let guides_nothing = !name.contains("bench");
        eprintln!(
            "\n#947 {name}: n = ({n_core}, {n_clad}), a = {a}, contrast-scaled floor = \
             {floor:.3e} ({:.0}× the PML floor)",
            floor / physical_curl_floor_pml()
        );
        for &mesh_size in meshes {
            let (mesh, tags, eps, interior) =
                pml_disk_913((n_core, n_clad), a, (clad_r, outer_r), mesh_size);
            let raw = dielectric_raw_candidates_p2_pml(
                &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 40, None,
            )
            .unwrap();
            let n_dof = n_dof_2d_nedelec2(&mesh);
            let interior_to_full: Vec<usize> = interior
                .iter()
                .enumerate()
                .filter_map(|(i, &keep)| keep.then_some(i))
                .collect();
            eprintln!("  mesh {mesh_size:?}, request 40:");
            // (β², curl, core fraction) of each converged in-window bound pair.
            let mut rows: Vec<(f64, f64, f64)> = Vec::new();
            for c in &raw.cands {
                let in_window = window.0 < c.beta_sq.re && c.beta_sq.re < window.1;
                if !(in_window && bound_c(c.beta_sq)) {
                    continue;
                }
                let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
                for (i, &f) in interior_to_full.iter().enumerate() {
                    e_edges[f] = c.vector[i];
                }
                let beta = principal_sqrt_c64(c.beta_sq);
                let mode = DielectricModePml {
                    n_eff: beta / c64::new(k0, 0.0),
                    beta,
                    beta_sq: c.beta_sq,
                    guided: true,
                    e_edges,
                };
                let cf = dielectric_mode_field_shape_pml(&mesh, &tags, &mode).core_energy_fraction;
                rows.push((c.beta_sq.re, c.curl_ratio, cf));
            }
            // The pairs the old floor keeps (the nullspace at `b ≈ 1` is
            // below both floors), most confined first.
            for &(re, curl, cf) in rows
                .iter()
                .filter(|r| r.1 > physical_curl_floor_pml())
                .take(8)
            {
                eprintln!(
                    "    converged  β² = {re:.5}  b = {:.4}  curl = {curl:.3e}  \
                     × floor = {:>7.3}  core frac = {cf:.3}  kept: old {} / new {}",
                    b_of(re),
                    curl / floor,
                    curl > physical_curl_floor_pml(),
                    curl > pml_classifier_curl_floor(e_max, e_min),
                );
            }
            let (lo, hi) = rows.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), r| {
                (lo.min(r.1 / floor), hi.max(r.1 / floor))
            });
            let kept_new = rows
                .iter()
                .filter(|r| r.1 > pml_classifier_curl_floor(e_max, e_min))
                .count();
            eprintln!(
                "    {} converged bound pair(s), × floor {lo:.3} … {hi:.3}; kept by old \
                 floor {}, by contrast-scaled floor {kept_new}",
                rows.len(),
                rows.iter()
                    .filter(|r| r.1 > physical_curl_floor_pml())
                    .count(),
            );
            for w in &raw.localized_withheld {
                let in_window = window.0 < w.beta_sq.re && w.beta_sq.re < window.1;
                if in_window && bound_c(w.beta_sq) {
                    eprintln!(
                        "    withheld   β² = {:.5}  b = {:.4}  curl = {:.3e}  × floor = {:>7.3}  \
                         ρ = {:.2e}",
                        w.beta_sq.re,
                        b_of(w.beta_sq.re),
                        w.curl_ratio,
                        w.curl_ratio / floor,
                        w.residual
                    );
                }
            }

            // The public call, retry included.
            take_low_curl_events();
            let public = solve_dielectric_modes2_pml(
                &mesh, &eps, &tags, &interior, clad_r, outer_r, 6.0, k0, 4,
            );
            let flags = take_low_curl_events();
            match public {
                Ok(modes) => {
                    let bound: Vec<&DielectricModePml> =
                        modes.iter().filter(|m| bound_c(m.beta_sq)).collect();
                    let summary: Vec<String> = bound
                        .iter()
                        .map(|m| format!("b = {:.4}", b_of(m.beta_sq.re)))
                        .collect();
                    eprintln!(
                        "    PUBLIC: Ok, {} mode(s), {} bound {summary:?}, {} low-curl \
                         flag(s)",
                        modes.len(),
                        bound.len(),
                        flags.len()
                    );
                    if name.starts_with("anti-guide") {
                        if !bound.is_empty() {
                            failures.push(format!(
                                "{name} {mesh_size:?}: ladder pair(s) returned as bound \
                                 modes: {summary:?}"
                            ));
                        }
                    } else if guides_nothing {
                        // Thin core: whatever is returned is the ladder;
                        // every returned bound mode must be flagged.
                        let unflagged = bound
                            .iter()
                            .filter(|m| !flags.iter().any(|f| (f.1 - m.beta_sq.re).abs() < 1e-9))
                            .count();
                        if unflagged > 0 {
                            failures.push(format!(
                                "{name} {mesh_size:?}: {unflagged} unflagged ladder \
                                 mode(s): {summary:?}"
                            ));
                        }
                    } else if let Some(top) = bound.first() {
                        if !flags.is_empty() {
                            failures.push(format!(
                                "{name} {mesh_size:?}: benchmark mode(s) flagged: {flags:?}"
                            ));
                        }
                        // The classifier's own curl ratio for the pick.
                        let pick = raw
                            .cands
                            .iter()
                            .find(|c| (c.beta_sq.re - top.beta_sq.re).abs() < 1e-9)
                            .map(|c| c.curl_ratio / floor);
                        eprintln!(
                            "    fundamental b = {:.4}, × floor = {}",
                            b_of(top.beta_sq.re),
                            pick.map_or("(not in the request-40 set)".into(), |r| format!(
                                "{r:.3}"
                            ))
                        );
                        if let Some(r) = pick {
                            fundamentals.push((name, mesh_size, r));
                        }
                    } else {
                        failures.push(format!("{name} {mesh_size:?}: no bound fundamental"));
                    }
                }
                Err(e) => {
                    eprintln!("    PUBLIC: Err({e})");
                    if !(guides_nothing && matches!(e, EigenError::SelectionHole { .. })) {
                        failures.push(format!("{name} {mesh_size:?}: unexpected error {e:?}"));
                    }
                }
            }
        }
    }
    eprintln!("\n#947 selected fundamentals (× contrast-scaled floor): {fundamentals:?}");
    for &(name, mesh_size, r) in &fundamentals {
        if r < 3.0 {
            failures.push(format!(
                "{name} {mesh_size:?}: fundamental at {r:.3}× the floor"
            ));
        }
    }
    assert_eq!(fundamentals.len(), 10, "{fundamentals:?}; {failures:#?}");
    assert!(failures.is_empty(), "{failures:#?}");
}
