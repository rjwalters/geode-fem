//! Spiral-inductor mesh fixture + tag-adapter regressions (Epic #193
//! Phase 3, issue #210).
//!
//! Two bundled fixtures share the physical-group convention of
//! `reference/gmsh/spiral_inductor.geo`:
//!
//! - **benchmark** (`spiral_3p5.msh`, ~54 k edges) — the Phase 3
//!   benchmark mesh for issue #211, assemblable since the sparse
//!   `[nnz]` Burn scatter landed (issue #218);
//! - **smoke** (`spiral_3p5_smoke.msh`, ~15 k edges) — same topology,
//!   coarser, used for the fast end-to-end solve.
//!
//! Coverage:
//!
//! 1. **Fixture round-trip** (both fixtures) — all expected physical
//!    groups (substrate / dielectric / air / air_buffer volumes; port /
//!    conductor_surface / outer_boundary surfaces), every tet
//!    region-tagged, positive signed volumes (no inverted tets), and
//!    the unique-edge count inside the ≤ 100 k direct-sparse-LU budget.
//! 2. **Surface-tag retention** (both) — tagged triangles survive
//!    loading (the issue's core mesh-I/O gap) and conform to the
//!    volume mesh: every tagged triangle edge appears in the global
//!    edge table, so the port mass/flux and Leontovich surface
//!    assemblies cannot panic.
//! 3. **Tag adapter** (both) — port faces map to a [`LumpedPort`]
//!    whose derived width/length match the generation parameters
//!    (w = 6 µm, g = 4 µm); conductor faces map to a
//!    [`SurfaceImpedanceBc`]; air-buffer tets and outer-boundary
//!    triangles provide the UPML region inputs.
//! 4. **End-to-end smoke** (smoke fixture) — one
//!    [`driven_solve_with_ports`] solve at 10 GHz (port-driven, PEC
//!    outer walls + PEC conductor cavity via the edge-exact mask)
//!    completes with a healthy direct-solve residual and finite,
//!    non-zero port V / I / Z_in.
//! 5. **Benchmark-fixture solve** (issue #218) — [`DrivenOperator`]
//!    assembly + one `solve_at` on the full 54 k-edge benchmark mesh
//!    (above the 46 340-edge i32 dense-scatter overflow threshold),
//!    port-driven with the conductor as a Leontovich (good-conductor)
//!    impedance surface — exercising both the `[nnz]` volume scatter
//!    and the triplet-based surface mass at scale.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::mesh::spiral::{
    CONDUCTOR_SIGMA_NATURAL, PHYS_AIR, PHYS_AIR_BUFFER, PHYS_CONDUCTOR_SURFACE, PHYS_DIELECTRIC,
    PHYS_OUTER_BOUNDARY, PHYS_PORT, PHYS_SUBSTRATE, PORT_E_HAT,
};
use geode_core::{
    driven_solve_with_ports, pec_interior_mask_from_triangles, port_current, port_input_impedance,
    port_voltage, read_spiral_fixture, read_spiral_smoke_fixture, CurrentSource, DefaultBackend,
    DrivenBcs, DrivenMaterials, DrivenOperator, SpiralFixture, SurfaceImpedanceBc,
    SurfaceImpedanceModel,
};
use std::collections::BTreeSet;

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn fixtures() -> [(&'static str, SpiralFixture); 2] {
    [
        (
            "benchmark",
            read_spiral_fixture().expect("benchmark spiral fixture must load"),
        ),
        (
            "smoke",
            read_spiral_smoke_fixture().expect("smoke spiral fixture must load"),
        ),
    ]
}

/// Generation parameters shared by both fixtures
/// (reference/gmsh/spiral_3p5_generic.yaml / spiral_3p5_smoke.yaml)
/// that the adapter re-derives from the mesh.
const TRACE_WIDTH: f64 = 6.0;
const PORT_GAP: f64 = 4.0;

/// ω = k₀ = 2πf/c at f = 10 GHz in the fixtures' micron length unit.
const OMEGA_10GHZ: f64 = 2.0 * std::f64::consts::PI * 10.0e9 / 2.99792458e14;

#[test]
fn fixture_roundtrip_counts_and_groups() {
    for (name, f) in fixtures() {
        assert!(f.mesh.n_nodes() > 0, "{name}: no nodes");
        assert!(f.mesh.n_tets() > 0, "{name}: no tets");
        assert_eq!(f.tet_physical_tags.len(), f.mesh.n_tets(), "{name}");
        assert_eq!(
            f.boundary_triangles.len(),
            f.triangle_physical_tags.len(),
            "{name}"
        );

        // All seven physical groups present with their canonical names.
        let want = [
            (3, PHYS_SUBSTRATE, "substrate"),
            (3, PHYS_DIELECTRIC, "dielectric"),
            (3, PHYS_AIR, "air"),
            (3, PHYS_AIR_BUFFER, "air_buffer"),
            (2, PHYS_PORT, "port"),
            (2, PHYS_CONDUCTOR_SURFACE, "conductor_surface"),
            (2, PHYS_OUTER_BOUNDARY, "outer_boundary"),
        ];
        for (dim, tag, gname) in want {
            assert_eq!(
                f.mesh.physical_groups.get(&(dim, tag)).map(String::as_str),
                Some(gname),
                "{name}: missing physical group ({dim}, {tag}) {gname:?}"
            );
        }

        // Every tet carries exactly one of the four region tags.
        let n_regions: usize = [PHYS_SUBSTRATE, PHYS_DIELECTRIC, PHYS_AIR, PHYS_AIR_BUFFER]
            .iter()
            .map(|&t| f.tets_with_tag(t).len())
            .sum();
        assert_eq!(
            n_regions,
            f.mesh.n_tets(),
            "{name}: every tet must be region-tagged"
        );

        // All three surface groups + the UPML buffer are populated.
        assert!(!f.port_triangles().is_empty(), "{name}: no port triangles");
        assert!(
            !f.conductor_triangles().is_empty(),
            "{name}: no conductor triangles"
        );
        assert!(
            !f.outer_boundary_triangles().is_empty(),
            "{name}: no outer-boundary triangles"
        );
        assert!(
            !f.air_buffer_tets().is_empty(),
            "{name}: no air-buffer (UPML) tets"
        );

        // Direct-sparse-LU affordability budget (issue #210).
        let n_edges = f.mesh.edges().len();
        assert!(
            n_edges <= 100_000,
            "{name}: {n_edges} edges exceeds the 100k benchmark budget"
        );
    }
}

#[test]
fn fixtures_have_no_inverted_tets() {
    for (name, f) in fixtures() {
        for (i, tet) in f.mesh.tets.iter().enumerate() {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| f.mesh.nodes[tet[k] as usize]);
            let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
            let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
            let e3 = [v[3][0] - v[0][0], v[3][1] - v[0][1], v[3][2] - v[0][2]];
            let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1])
                - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
                + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
            assert!(
                det > 0.0,
                "{name}: tet {i} has non-positive signed volume {det}"
            );
        }
    }
}

/// Every tagged triangle must be a conforming face of the volume mesh:
/// its three edges appear in the global edge table. This is the
/// precondition for the port surface-mass/flux and Leontovich surface
/// assemblies (which panic on a missing edge).
#[test]
fn tagged_triangles_conform_to_volume_mesh() {
    for (name, f) in fixtures() {
        let edge_set: BTreeSet<(u32, u32)> =
            f.mesh.edges().into_iter().map(|e| (e[0], e[1])).collect();
        for (tri, tag) in f
            .boundary_triangles
            .iter()
            .zip(f.triangle_physical_tags.iter())
        {
            for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                assert!(
                    edge_set.contains(&(lo, hi)),
                    "{name}: triangle {tri:?} (tag {tag}) edge ({lo}, {hi}) \
                     missing from edge table"
                );
            }
        }
    }
}

#[test]
fn port_adapter_recovers_generation_parameters() {
    for (name, f) in fixtures() {
        let port = f.port();

        assert_eq!(port.e_hat, PORT_E_HAT, "{name}");
        assert!(
            (port.length - PORT_GAP).abs() < 1e-9,
            "{name}: port gap length {} != generation parameter {PORT_GAP}",
            port.length
        );
        assert!(
            (port.width - TRACE_WIDTH).abs() < 1e-9,
            "{name}: port width {} != trace width {TRACE_WIDTH}",
            port.width
        );

        // R = 50 Ω in units of η₀ maps to Z_s = R·w/l.
        let r_50 = 50.0 / 376.730_313_668;
        let lp = port.lumped_port(r_50, c64::new(1.0, 0.0));
        assert!(
            (lp.surface_impedance() - r_50 * TRACE_WIDTH / PORT_GAP).abs() < 1e-12,
            "{name}"
        );
    }
}

#[test]
fn conductor_adapter_builds_leontovich_surface() {
    let f = read_spiral_smoke_fixture().expect("fixture");
    let cond = f.conductor_triangles();
    let bc = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
        },
    };
    // The good-conductor coefficient iω/Z_s(ω) must be finite and
    // non-singular at the benchmark frequency.
    let coeff = bc
        .model
        .weak_coefficient(OMEGA_10GHZ)
        .expect("copper surface impedance must be regular at 10 GHz");
    assert!(coeff.re.is_finite() && coeff.im.is_finite());
    assert!(coeff.norm() > 0.0);
}

/// End-to-end smoke (acceptance criterion 3 of issue #210): one
/// port-driven [`driven_solve_with_ports`] solve on the smoke fixture.
///
/// Setup: lossy substrate/dielectric permittivities from the recorded
/// stack materials, PEC outer walls and PEC conductor cavity through
/// the edge-exact mask (the Leontovich variant swaps the conductor
/// entries of the mask for a [`SurfaceImpedanceBc`]), 50 Ω port driven
/// with V_inc = 1 at 10 GHz, no volume current.
#[test]
fn driven_solve_with_ports_end_to_end_smoke() {
    let f = read_spiral_smoke_fixture().expect("fixture");
    let edges = f.mesh.edges();

    let eps = f.epsilon_r_default();

    let outer = f.outer_boundary_triangles();
    let cond = f.conductor_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice(), cond.as_slice()]);
    let n_pec = mask.iter().filter(|&&keep| !keep).count();
    assert!(n_pec > 0, "PEC mask must eliminate boundary edges");
    assert!(
        mask.iter().any(|&keep| keep),
        "PEC mask must keep interior edges"
    );

    let port = f.port();
    let r_50 = 50.0 / 376.730_313_668;
    let lp = port.lumped_port(r_50, c64::new(1.0, 0.0));

    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; f.mesh.n_tets()],
    };

    let sol = driven_solve_with_ports::<B>(
        &f.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        OMEGA_10GHZ,
        &source,
        &device(),
    )
    .expect("port-driven solve on the spiral smoke fixture must succeed");

    assert!(
        sol.residual_rel < 1e-8,
        "direct-solve residual {} not at round-off floor",
        sol.residual_rel
    );

    let v = port_voltage(&f.mesh, &lp, &edges, &sol.e_edges);
    let i = port_current(&lp, v);
    let z = port_input_impedance(&f.mesh, &lp, &edges, &sol.e_edges);

    for (name, val) in [("V", v), ("I", i), ("Z_in", z)] {
        assert!(
            val.re.is_finite() && val.im.is_finite(),
            "{name} = {val} is not finite"
        );
    }
    assert!(v.norm() > 0.0, "port voltage must be non-zero when driven");
    assert!(i.norm() > 0.0, "port current must be non-zero when driven");

    // The spiral is a sub-wavelength inductive loop behind a resistive
    // source: |Z_in| should be a sane, non-degenerate impedance (not
    // collapsed to 0, not blown up) — a loose physical sanity band, not
    // a benchmark assertion (that is issue #211's job).
    let z_ohms = z * 376.730_313_668;
    assert!(
        z_ohms.norm() > 1e-3 && z_ohms.norm() < 1e6,
        "Z_in = {z_ohms} Ω outside loose sanity band"
    );
}

/// Benchmark-fixture solve (issue #218 acceptance): [`DrivenOperator`]
/// assembly + one `solve_at` on the full 54,428-edge spiral mesh.
///
/// This mesh sits **above** the 46,340-edge threshold where the old
/// dense `[n_edges²]` Burn scatter overflowed its i32 linear index
/// (debug-build panic) and would have needed ~12 GB per f32 tensor
/// (~24 GB per dense f64 faer intermediate) — the sparse `[nnz]`
/// pattern-slot assembly handles it in O(nnz) ≈ 1 M values per matrix.
///
/// Setup mirrors the smoke solve, with one upgrade: the conductor
/// surface is a Leontovich good-conductor impedance BC instead of a
/// PEC cavity, so the triplet-based surface mass (the second dense
/// `[n_edges²]` object eliminated by #218) is exercised at scale too.
/// PEC outer walls, 50 Ω port driven with V_inc = 1 at 10 GHz, no
/// volume current.
#[test]
fn driven_operator_assembles_and_solves_benchmark_fixture() {
    let f = read_spiral_fixture().expect("benchmark fixture must load");
    let edges = f.mesh.edges();
    assert!(
        edges.len() > 46_340,
        "benchmark fixture has {} edges — must exceed the i32 dense-scatter \
         overflow threshold for this regression to bite",
        edges.len()
    );

    let eps = f.epsilon_r_default();

    // PEC outer walls only — the conductor surface gets the impedance BC.
    let outer = f.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    assert!(mask.iter().any(|&keep| !keep) && mask.iter().any(|&keep| keep));

    let cond = f.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
        },
    };

    let port = f.port();
    let r_50 = 50.0 / 376.730_313_668;
    let lp = port.lumped_port(r_50, c64::new(1.0, 0.0));

    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; f.mesh.n_tets()],
    };

    let op = DrivenOperator::assemble::<B>(
        &f.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &source,
        &device(),
    )
    .expect("sparse assembly on the 54k-edge benchmark fixture must succeed");

    let sol = op
        .solve_at(OMEGA_10GHZ)
        .expect("port-driven solve on the benchmark fixture must succeed");

    assert!(
        sol.residual_rel < 1e-8,
        "direct-solve residual {} not at round-off floor",
        sol.residual_rel
    );
    assert!(sol
        .e_edges
        .iter()
        .all(|e| e.re.is_finite() && e.im.is_finite()));

    let v = op.port_voltage(0, &sol.e_edges);
    let i = op.port_current(0, v);
    assert!(v.re.is_finite() && v.im.is_finite(), "V = {v} not finite");
    assert!(i.re.is_finite() && i.im.is_finite(), "I = {i} not finite");
    assert!(v.norm() > 0.0, "port voltage must be non-zero when driven");
    assert!(i.norm() > 0.0, "port current must be non-zero when driven");

    let z_ohms = (v / i) * 376.730_313_668;
    assert!(
        z_ohms.norm() > 1e-3 && z_ohms.norm() < 1e6,
        "Z_in = {z_ohms} Ω outside loose sanity band"
    );
}
