//! Issue #725: a surface / port triangle that is not a face of any tet must
//! come back from every public entry point that accepts caller-supplied
//! surface triangles as a typed `SurfaceNotOnMesh` error, never a panic in
//! the Whitney surface kernels.
//!
//! Fixture: the 2×2×2 cube mesh plus one extra node that belongs to no tet
//! (the PR #724 incident shape — a dangling sheet piece whose triangles use
//! nodes no tet touches). The offending list mixes one genuine tet face with
//! the dangling triangle so the reported counts (1 of 2) are exercised.
//!
//! Issue #732 adds a second, distinct fixture below: a "false face" — a
//! triangle whose three edges *all* exist in the mesh (each edge belongs to
//! some tet) but which is not itself a face of any single tet. This is the
//! case the stricter per-tet-face check added in #725/PR #729 exists for,
//! as opposed to the dangling/missing-edge case above.

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::driven::ports::{
    LumpedPort, PortMedium, PortMode, WavePort, solve_wave_port_sweep, waveguide_mode_reduce,
};
use geode_core::driven::shape::{
    driven_shape_gradient_matched_upml_ports, driven_shape_gradient_moving_port_s11,
    driven_shape_gradient_ports_complex,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
    SurfaceImpedanceModel, driven_solve_with_ports, driven_solve_with_surface_impedance,
};
use geode_core::eigen::sensitivity::{EigenSensitivity, EigenSensitivityError};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

struct Fixture {
    mesh: TetMesh,
    interior: Vec<bool>,
    eps: Vec<c64>,
    source: CurrentSource,
    /// One genuine tet face followed by one dangling triangle.
    faces: Vec<[u32; 3]>,
    dangling: [u32; 3],
}

fn fixture() -> Fixture {
    let mut mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let t0 = mesh.tets[0];
    let extra = mesh.nodes.len() as u32;
    mesh.nodes.push([2.0, 2.0, 2.0]);
    let dangling = [t0[0], t0[1], extra];
    let faces = vec![[t0[0], t0[1], t0[2]], dangling];
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]; mesh.n_tets()],
    };
    Fixture {
        mesh,
        interior,
        eps,
        source,
        faces,
        dangling,
    }
}

fn port(faces: &[[u32; 3]]) -> LumpedPort<'_> {
    LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    }
}

fn zero_objective(x: &[c64]) -> (f64, Vec<c64>) {
    (0.0, vec![c64::new(0.0, 0.0); x.len()])
}

#[track_caller]
fn assert_driven_not_on_mesh<T>(res: Result<T, DrivenError>, want_surface: &str, tri: [u32; 3]) {
    match res {
        Err(DrivenError::SurfaceNotOnMesh {
            surface,
            triangle,
            dangling,
            total,
        }) => {
            assert_eq!(surface, want_surface);
            assert_eq!(triangle, tri);
            assert_eq!((dangling, total), (1, 2));
        }
        Err(other) => panic!("expected SurfaceNotOnMesh, got {other}"),
        Ok(_) => panic!("expected SurfaceNotOnMesh, got Ok"),
    }
}

#[test]
fn error_message_names_surface_and_counts() {
    let err = DrivenError::SurfaceNotOnMesh {
        surface: "lumped port 0".to_string(),
        triangle: [1, 2, 3],
        dangling: 36,
        total: 40,
    };
    let msg = err.to_string();
    assert!(msg.contains("lumped port 0"), "{msg}");
    assert!(msg.contains("[1, 2, 3]"), "{msg}");
    assert!(msg.contains("36 of 40"), "{msg}");
}

/// Rows 1–2: `DrivenOperator::assemble` and the `driven_solve*` wrappers.
#[test]
fn driven_operator_rejects_dangling_impedance_surface_and_port() {
    let f = fixture();
    let bcs = DrivenBcs {
        pec_interior_mask: &f.interior,
    };
    let good_surface = [SurfaceImpedanceBc {
        triangles: &f.faces[..1],
        model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
    }];
    let surfaces = [
        good_surface[0],
        SurfaceImpedanceBc {
            triangles: &f.faces,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        },
    ];
    assert_driven_not_on_mesh(
        DrivenOperator::assemble::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &[],
            &surfaces,
            &f.source,
            &device(),
        ),
        "impedance surface 1",
        f.dangling,
    );
    assert_driven_not_on_mesh(
        driven_solve_with_surface_impedance::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &surfaces,
            1.0,
            &f.source,
            &device(),
        ),
        "impedance surface 1",
        f.dangling,
    );

    let good_port = port(&f.faces[..1]);
    let ports = [good_port, port(&f.faces)];
    assert_driven_not_on_mesh(
        DrivenOperator::assemble::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &ports,
            &[],
            &f.source,
            &device(),
        ),
        "lumped port 1",
        f.dangling,
    );
    assert_driven_not_on_mesh(
        driven_solve_with_ports::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &ports,
            1.0,
            &f.source,
            &device(),
        ),
        "lumped port 1",
        f.dangling,
    );
}

/// Rows 3–5: the three lumped-port shape-gradient entry points.
#[test]
fn shape_gradients_reject_dangling_port() {
    let f = fixture();
    let bcs = DrivenBcs {
        pec_interior_mask: &f.interior,
    };
    let bad = port(&f.faces);

    assert_driven_not_on_mesh(
        driven_shape_gradient_ports_complex::<B, _>(
            &f.mesh,
            &f.eps,
            &bcs,
            1.0,
            &f.source,
            std::slice::from_ref(&bad),
            zero_objective,
            &device(),
        ),
        "lumped port 0",
        f.dangling,
    );

    assert_driven_not_on_mesh(
        driven_shape_gradient_moving_port_s11::<B>(
            &f.mesh,
            &f.eps,
            &bcs,
            1.0,
            &f.source,
            &bad,
            1.0,
            &device(),
        ),
        "lumped port 0",
        f.dangling,
    );

    let one = c64::new(1.0, 0.0);
    let zero = c64::new(0.0, 0.0);
    let identity = [[one, zero, zero], [zero, one, zero], [zero, zero, one]];
    let tensor = vec![identity; f.mesh.n_tets()];
    assert_driven_not_on_mesh(
        driven_shape_gradient_matched_upml_ports::<B, _>(
            &f.mesh,
            &tensor,
            &tensor,
            &bcs,
            std::slice::from_ref(&bad),
            1.0,
            &f.source,
            zero_objective,
            &device(),
        ),
        "lumped port 0",
        f.dangling,
    );
}

/// Row 6: the wave-port sweep and the modal projection.
#[test]
fn wave_port_paths_reject_dangling_port() {
    let f = fixture();
    let bcs = DrivenBcs {
        pec_interior_mask: &f.interior,
    };
    let edges = f.mesh.edges();
    let mode = PortMode {
        mode: vec![0.0; edges.len()],
        k_c: 1.0,
        a_inc: c64::new(1.0, 0.0),
    };
    let ports = [WavePort {
        faces: f.faces.clone(),
        modes: vec![mode],
        medium: PortMedium::VACUUM,
    }];

    assert_driven_not_on_mesh(
        solve_wave_port_sweep::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &ports,
            &[1.0],
            &device(),
        ),
        "wave port 0",
        f.dangling,
    );

    let e_edges = vec![c64::new(0.0, 0.0); edges.len()];
    assert_driven_not_on_mesh(
        waveguide_mode_reduce(&f.mesh, &ports, &edges, &e_edges),
        "wave port 0",
        f.dangling,
    );
}

/// Row 7: the London penetration-depth eigen-sensitivity.
#[test]
fn eigen_london_sensitivity_rejects_dangling_wall() {
    let f = fixture();
    let edges = f.mesh.edges();
    let dim = f.interior.iter().filter(|&&k| k).count();
    let eigenvector = vec![1.0; dim];
    let eps_r = vec![1.0; f.mesh.n_tets()];
    let lambdas = [1.0, 2.0];
    let ctx = EigenSensitivity {
        mesh: &f.mesh,
        edges: &edges,
        interior_mask: &f.interior,
        eps_r: &eps_r,
        lambdas: &lambdas,
        mode_index: 0,
        eigenvector: &eigenvector,
        min_rel_gap: 1e-6,
    };
    let err = ctx
        .deigenvalue_dlambda_l(&f.faces, 0.1)
        .expect_err("dangling London wall must be rejected");
    assert_eq!(
        err,
        EigenSensitivityError::SurfaceNotOnMesh {
            triangle: f.dangling,
            dangling: 1,
            total: 2,
        }
    );
    assert!(err.to_string().contains("1 of 2"), "{err}");

    // A conforming wall still evaluates.
    ctx.deigenvalue_dlambda_l(&f.faces[..1], 0.1)
        .expect("conforming London wall");
}

// ---------------------------------------------------------------------------
// Issue #732: the "false face" case — all three edges of the surface
// triangle exist in the mesh, but the triangle itself is not a face of any
// single tet.
// ---------------------------------------------------------------------------

/// Three tets, six nodes, each tet contributing exactly one edge of the
/// target triangle `{A, B, C}` plus two satellite nodes shared pairwise with
/// the other two tets (the octahedron's three tets fanned off its
/// equatorial triangle, built directly rather than derived from a mesh
/// generator):
///
/// ```text
/// nodes: A, B, C   (the target "false face" triangle)
///        D, E, F   (satellites)
///
/// tetX = [A, B, D, E]   -- contributes edge A-B
/// tetY = [B, C, D, F]   -- contributes edge B-C
/// tetZ = [A, C, E, F]   -- contributes edge A-C
/// ```
///
/// `A-B` is an edge of `tetX`, `B-C` of `tetY`, `A-C` of `tetZ` — all three
/// edges of triangle `{A, B, C}` exist in the mesh. But no single tet
/// contains all of `A`, `B`, and `C` (`tetX` lacks `C`, `tetY` lacks `A`,
/// `tetZ` lacks `B`), so `{A, B, C}` is not a tet face.
struct FalseFaceFixture {
    mesh: TetMesh,
    interior: Vec<bool>,
    eps: Vec<c64>,
    source: CurrentSource,
    /// The false-face triangle `{A, B, C}`.
    false_face: [u32; 3],
}

fn false_face_fixture() -> FalseFaceFixture {
    let nodes = vec![
        [0.0, 0.0, 1.0],   // A
        [1.0, 0.0, -0.5],  // B
        [-1.0, 0.0, -0.5], // C
        [0.0, 1.0, 0.0],   // D
        [0.6, -0.8, 0.3],  // E
        [-0.6, -0.8, 0.3], // F
    ];
    let (a, b, c, d, e, f) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let tets = vec![
        [a, b, d, e], // tetX: contributes edge A-B
        [b, c, d, f], // tetY: contributes edge B-C
        [a, c, e, f], // tetZ: contributes edge A-C
    ];
    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: Default::default(),
    };
    let n_edges = mesh.edges().len();
    let interior = vec![true; n_edges];
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]; mesh.n_tets()],
    };
    FalseFaceFixture {
        mesh,
        interior,
        eps,
        source,
        false_face: [a, b, c],
    }
}

#[track_caller]
fn assert_false_face_not_on_mesh<T>(
    res: Result<T, DrivenError>,
    want_surface: &str,
    tri: [u32; 3],
) {
    match res {
        Err(DrivenError::SurfaceNotOnMesh {
            surface,
            triangle,
            dangling,
            total,
        }) => {
            assert_eq!(surface, want_surface);
            assert_eq!(triangle, tri);
            // The false face is the only triangle in its list: 1 of 1.
            assert_eq!((dangling, total), (1, 1));
        }
        Err(other) => panic!("expected SurfaceNotOnMesh, got {other}"),
        Ok(_) => panic!("expected SurfaceNotOnMesh, got Ok"),
    }
}

/// Sanity check on the fixture itself: a weaker *edge-only* membership test
/// (does each edge of the triangle exist anywhere in the mesh's edge table?)
/// must pass, to prove this fixture exercises the stricter per-tet-face
/// check — not the dangling/missing-edge case already covered above.
#[test]
fn false_face_triangle_edges_all_exist_in_mesh() {
    let f = false_face_fixture();
    let edges = f.mesh.edges();
    let [a, b, c] = f.false_face;
    let edge_key = |u: u32, v: u32| if u < v { [u, v] } else { [v, u] };
    for (u, v) in [(a, b), (b, c), (a, c)] {
        assert!(
            edges.contains(&edge_key(u, v)),
            "edge ({u}, {v}) of the false face must already exist in the \
             mesh's edge table — otherwise this fixture would only exercise \
             the dangling/missing-edge case, not the false-face case"
        );
    }
}

/// `DrivenOperator::assemble` rejects the false face with a typed
/// `SurfaceNotOnMesh` error (never a panic), through both an impedance
/// surface and a lumped port.
#[test]
fn driven_operator_rejects_false_face_impedance_surface_and_port() {
    let f = false_face_fixture();
    let bcs = DrivenBcs {
        pec_interior_mask: &f.interior,
    };

    let surfaces = [SurfaceImpedanceBc {
        triangles: std::slice::from_ref(&f.false_face),
        model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
    }];
    assert_false_face_not_on_mesh(
        DrivenOperator::assemble::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &[],
            &surfaces,
            &f.source,
            &device(),
        ),
        "impedance surface 0",
        f.false_face,
    );

    let bad_port = port(std::slice::from_ref(&f.false_face));
    assert_false_face_not_on_mesh(
        DrivenOperator::assemble::<B>(
            &f.mesh,
            DrivenMaterials::Scalar(&f.eps),
            None,
            &bcs,
            &[bad_port],
            &[],
            &f.source,
            &device(),
        ),
        "lumped port 0",
        f.false_face,
    );
}
