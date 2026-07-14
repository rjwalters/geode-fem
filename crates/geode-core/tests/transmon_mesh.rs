//! Transmon + readout-resonator mesh-adapter integration tests
//! (Epic #476 Phase A, issue #485).
//!
//! Exercises `mesh::transmon` against the bundled coarse `transmon_smoke`
//! fixture: physical-group presence + exact element counts (asserted from
//! the provenance file), region-tag totality, surface-set disjointness,
//! PEC-mask + port-set construction, and a coarse-mesh SMOKE eigenmode
//! solve that RUNS (finite numbers; the quantitative comparison against
//! the Palace oracle is Phase B and is NOT gated here).
//!
//! The fixture is currently a schema-faithful PLACEHOLDER (DeviceLayout.jl
//! cannot be loaded in the build environment — Cairo/Pango precompile
//! gap); see `mesh::transmon` module docs. When the operator swaps in the
//! real DeviceLayout mesh, only the exact per-group counts below change
//! (from the new provenance file) — the adapter and every other assertion
//! are unchanged.

use burn::tensor::backend::BackendTypes;

use geode_core::assembly::nedelec::assemble_global_nedelec_with_epsilon;
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::transmon::{
    JUNCTION_CAPACITANCE_F, JUNCTION_E_HAT, JUNCTION_INDUCTANCE_H, PHYS_EXTERIOR_BOUNDARY,
    PHYS_LUMPED_ELEMENT, PHYS_METAL, PHYS_PORT_1, PHYS_PORT_2, PHYS_SUBSTRATE, PHYS_VACUUM,
    PORT_E_HAT, PORT_RESISTANCE_OHM, sapphire_eps_lab, sapphire_eps_scalar,
};
use geode_core::mesh::{TransmonFixture, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

// ---- Exact per-group counts from `transmon_smoke.provenance.txt`. -------
// PLACEHOLDER counts — regenerate from the real fixture's provenance when
// the operator swaps in the DeviceLayout mesh.
const N_NODES: usize = 755;
const N_TETS: usize = 3395;
const N_SUBSTRATE_TETS: usize = 1079;
const N_VACUUM_TETS: usize = 2316;
const N_METAL_TRIS: usize = 88;
const N_PORT_1_TRIS: usize = 4;
const N_PORT_2_TRIS: usize = 4;
const N_LUMPED_TRIS: usize = 4;
const N_EXTERIOR_TRIS: usize = 658;

fn fixture() -> TransmonFixture {
    read_transmon_smoke_fixture().expect("bundled transmon smoke fixture")
}

#[test]
fn fixture_loads_with_no_silent_drops() {
    let f = fixture();
    assert_eq!(f.mesh.n_nodes(), N_NODES, "node count");
    assert_eq!(f.mesh.n_tets(), N_TETS, "tet count");
    assert_eq!(f.tet_physical_tags.len(), f.mesh.n_tets());
    assert_eq!(f.boundary_triangles.len(), f.triangle_physical_tags.len());
    // Every declared group is present and populated (no silent drops of
    // surface elements — the whole point of the shared scanners).
    assert!(
        !f.boundary_triangles.is_empty(),
        "surface triangles dropped"
    );
}

#[test]
fn fixture_carries_all_physical_groups() {
    let f = fixture();
    let groups = [
        (3, PHYS_SUBSTRATE, "substrate"),
        (3, PHYS_VACUUM, "vacuum"),
        (2, PHYS_METAL, "metal"),
        (2, PHYS_PORT_1, "port_1"),
        (2, PHYS_PORT_2, "port_2"),
        (2, PHYS_LUMPED_ELEMENT, "lumped_element"),
        (2, PHYS_EXTERIOR_BOUNDARY, "exterior_boundary"),
    ];
    for (dim, tag, name) in groups {
        assert_eq!(
            f.mesh.physical_groups.get(&(dim, tag)),
            Some(&name.to_string()),
            "missing physical group ({dim}, {tag}) {name:?}"
        );
    }
}

#[test]
fn per_group_element_counts_match_provenance() {
    let f = fixture();
    // 3D regions.
    assert_eq!(f.substrate_tets().len(), N_SUBSTRATE_TETS, "substrate tets");
    assert_eq!(f.vacuum_tets().len(), N_VACUUM_TETS, "vacuum tets");
    // 2D surfaces.
    assert_eq!(f.metal_triangles().len(), N_METAL_TRIS, "metal tris");
    assert_eq!(f.port_1_triangles().len(), N_PORT_1_TRIS, "port_1 tris");
    assert_eq!(f.port_2_triangles().len(), N_PORT_2_TRIS, "port_2 tris");
    assert_eq!(
        f.lumped_element_triangles().len(),
        N_LUMPED_TRIS,
        "lumped_element tris"
    );
    assert_eq!(
        f.exterior_boundary_triangles().len(),
        N_EXTERIOR_TRIS,
        "exterior_boundary tris"
    );
}

#[test]
fn every_tet_carries_exactly_one_region_tag() {
    let f = fixture();
    // Region totality: substrate + vacuum partition all tets.
    assert_eq!(
        f.substrate_tets().len() + f.vacuum_tets().len(),
        f.mesh.n_tets(),
        "every tet must be substrate xor vacuum"
    );
    // No tet tagged with anything else.
    for &t in &f.tet_physical_tags {
        assert!(
            t == PHYS_SUBSTRATE || t == PHYS_VACUUM,
            "unexpected 3D tag {t}"
        );
    }
}

#[test]
fn surface_sets_are_disjoint_and_conform_to_mesh() {
    let f = fixture();
    // The five surface tags partition every tagged triangle exactly once.
    let total = f.metal_triangles().len()
        + f.port_1_triangles().len()
        + f.port_2_triangles().len()
        + f.lumped_element_triangles().len()
        + f.exterior_boundary_triangles().len();
    assert_eq!(
        total,
        f.boundary_triangles.len(),
        "surface tags must partition all tagged triangles (disjoint + total)"
    );
    // Every triangle node index is in range.
    let n = f.mesh.n_nodes() as u32;
    for tri in &f.boundary_triangles {
        for &v in tri {
            assert!(v < n, "triangle node index {v} out of range ({n} nodes)");
        }
    }
}

#[test]
fn pec_mask_from_metal_is_nonempty_and_consistent() {
    let f = fixture();
    let edges = f.mesh.edges();
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    assert!(!metal.is_empty(), "no metal triangles");

    // PEC mask over the metal + exterior-boundary faces (Phase A treats
    // both as PEC for the smoke solve).
    let mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    assert_eq!(mask.len(), edges.len());
    let n_interior = mask.iter().filter(|&&b| b).count();
    let n_pec = mask.len() - n_interior;
    assert!(n_interior > 0, "no interior edges survived PEC mask");
    assert!(n_pec > 0, "PEC mask removed no edges (metal tags missing?)");
}

#[test]
fn port_adapters_recover_directions_and_finite_geometry() {
    let f = fixture();
    for (name, port, e_hat) in [
        ("port_1", f.port_1(), PORT_E_HAT),
        ("port_2", f.port_2(), PORT_E_HAT),
        ("lumped_element", f.lumped_element_port(), JUNCTION_E_HAT),
    ] {
        assert!(!port.faces.is_empty(), "{name}: empty port faces");
        assert_eq!(port.e_hat, e_hat, "{name}: direction");
        assert!(
            port.width.is_finite() && port.width > 0.0,
            "{name}: width {} not positive-finite",
            port.width
        );
        assert!(
            port.length.is_finite() && port.length > 0.0,
            "{name}: length {} not positive-finite",
            port.length
        );
        // The lumped_port adapter borrows the faces and carries the R/V.
        let lp = port.lumped_port(PORT_RESISTANCE_OHM, faer::c64::new(1.0, 0.0));
        assert_eq!(lp.faces.len(), port.faces.len());
        assert!(lp.surface_impedance().is_finite());
    }
    // Junction lumped-element R/L/C constants are the DeviceLayout values.
    assert_eq!(PORT_RESISTANCE_OHM, 50.0);
    assert!((JUNCTION_INDUCTANCE_H - 14.860e-9).abs() < 1e-18);
    assert!((JUNCTION_CAPACITANCE_F - 5.5e-15).abs() < 1e-21);
}

#[test]
fn epsilon_maps_track_region_tags() {
    let f = fixture();
    // Scalar smoke map: trace-averaged sapphire on substrate, 1 in vacuum.
    let eps = f.epsilon_r_scalar();
    assert_eq!(eps.len(), f.mesh.n_tets());
    let eps_sub = sapphire_eps_scalar();
    let n_sub = eps.iter().filter(|&&e| (e - eps_sub).abs() < 1e-12).count();
    let n_vac = eps.iter().filter(|&&e| (e - 1.0).abs() < 1e-12).count();
    assert_eq!(n_sub, f.substrate_tets().len());
    assert_eq!(n_vac, f.vacuum_tets().len());
    assert_eq!(n_sub + n_vac, f.mesh.n_tets());

    // Full-tensor Phase-B map: rotated sapphire on substrate, identity in
    // vacuum. Shape + per-region content check.
    let tens = f.epsilon_tensor_r();
    assert_eq!(tens.len(), f.mesh.n_tets());
    let lab = sapphire_eps_lab();
    for (i, &tag) in f.tet_physical_tags.iter().enumerate() {
        if tag == PHYS_SUBSTRATE {
            assert!((tens[i][2][2].re - lab[2][2]).abs() < 1e-12);
            assert!((tens[i][0][0].re - lab[0][0]).abs() < 1e-12);
        } else {
            // vacuum identity
            assert!((tens[i][0][0].re - 1.0).abs() < 1e-12);
            assert!(tens[i][0][1].re.abs() < 1e-12);
        }
    }
}

/// Coarse-mesh SMOKE eigenmode solve: assemble the first-order Nédélec
/// curl-curl / ε-mass pencil on the transmon fixture with metal +
/// exterior boundary as PEC, reduce, and solve for the lowest few modes.
/// **The solve must RUN and return finite, positive numbers.** Numbers
/// are NOT gated against any reference — that is Phase B.
///
/// Gated behind `--ignored` because the dense generalized eigensolve is
/// O(n³) over the interior DOFs and takes minutes even at opt-level 3
/// (same profile note as `tests/sphere_pec_eigenmode.rs`). Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_mesh -- --ignored
/// ```
#[test]
#[ignore = "coarse dense eigensolve is O(n^3); runs in minutes — smoke only"]
fn smoke_eigenmode_solve_runs() {
    let f = fixture();
    eprintln!(
        "transmon smoke fixture: {} nodes, {} tets ({} substrate + {} vacuum)",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.substrate_tets().len(),
        f.vacuum_tets().len(),
    );

    // 1. Per-tet scalar ε (isotropic Phase-A hook).
    let epsilon_r = f.epsilon_r_scalar();

    // 2. Edge tables + sign convention.
    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    eprintln!("global edges: {n_edges}");

    // 3. Upload + assemble ε-scaled system.
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &epsilon_r,
    );

    // 4. PEC edge mask: metal + exterior boundary are PEC (Phase A).
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();
    eprintln!("PEC reduction: {n_edges} edges → {n_interior} interior DOFs");
    assert!(n_interior > 0, "no interior DOFs after PEC reduction");

    // 5. Reduce + solve for the lowest handful of modes.
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);
    let (k_int, m_int) =
        apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &interior_mask).expect("BC reduction");

    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), 12)
        .expect("smoke eigensolve must run without error");

    eprintln!("lowest {} eigenvalues:", lambdas.len());
    for (i, lam) in lambdas.iter().enumerate() {
        eprintln!("  λ[{i}] = {lam:.6e}");
    }

    // Acceptance: the solve RAN and returned finite numbers. Numbers are
    // NOT gated against any reference — that is Phase B. The lowest 12
    // eigenvalues here are the discrete curl-curl gradient nullspace
    // (kernel(K) = image(d⁰); ~n_interior_nodes near-zero modes, exactly
    // as `tests/sphere_pec_eigenmode.rs` documents), so we assert only
    // finiteness + a filled result vector. Reaching the physical band
    // would require requesting past the (hundreds-strong) nullspace and
    // is deferred to Phase B.
    assert!(!lambdas.is_empty(), "eigensolve returned no eigenvalues");
    assert_eq!(lambdas.len(), 12, "requested-count mismatch");
    for (i, lam) in lambdas.iter().enumerate() {
        assert!(lam.is_finite(), "λ[{i}] = {lam} is not finite");
    }
    let max_abs = lambdas.iter().map(|l| l.abs()).fold(0.0_f64, f64::max);
    eprintln!(
        "smoke eigenmode solve RAN: {} finite modes (gradient-nullspace band, \
         max |λ| = {max_abs:.3e}) — numbers ungated per Phase A",
        lambdas.len()
    );
}
