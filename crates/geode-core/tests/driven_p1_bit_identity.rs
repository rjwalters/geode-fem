//! p=1 **bit-identity** goldens for the order-pluggable driven path
//! (issue #838, Epic #836 Phase 1a).
//!
//! #838 re-keys [`DrivenOperator`] on an order-pluggable `HcurlSpace`. Its
//! hard requirement is that p=1 results do not move **at all**. This target
//! pins that with FNV-1a hashes of the raw IEEE-754 bit patterns of
//! representative p=1 outputs, recorded on `main` at b833d1a (v0.8.0)
//! **before** any #838 change:
//!
//! - the single-ω entry points (scalar complex ε, σ damping, a lumped port,
//!   a Leontovich good-conductor surface, a diagonal-tensor ε, a matched
//!   UPML full-tensor material, the degree-2 quadrature source);
//! - the dense lumped-port frequency sweep (`Z(ω)`) and the N-port
//!   S-parameter sweep (`Z` matrix);
//! - the adaptive PROM sweep (`Z(ω)` on the reduced model).
//!
//! A hash mismatch means a p=1 bit moved. Each case prints its hash, so a
//! deliberate future change can re-bless the table, but #838 must not.
//!
//! **Platform scope.** Bit patterns depend on the platform's `libm` and on
//! faer's SIMD kernels (NEON vs AVX), so the table is the one recorded on
//! the platform it was blessed on (macOS aarch64) and is **asserted only
//! there**; elsewhere every case still runs and prints its hash. The
//! platform-independent half of the guarantee is
//! `tests/driven_p2_operator.rs::p1_space_path_is_bit_identical_to_assemble`,
//! which compares the pre-#838 entry points against the order-generic
//! `assemble_with_space` p=1 arm inside one binary, on every platform.

use faer::c64;
use geode_core::driven::extraction::{driven_frequency_sweep, s_parameter_frequency_sweep};
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::rom::{RomSettings, rom_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, QuadCurrentSource, SurfaceImpedanceBc,
    SurfaceImpedanceModel, driven_solve_quad, driven_solve_with_ports, driven_solve_with_sigma,
    driven_solve_with_surface_impedance,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// FNV-1a over the bit patterns of a complex vector (re then im).
fn hash_c(v: &[c64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for z in v {
        for bits in [z.re.to_bits(), z.im.to_bits()] {
            for byte in bits.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// The platform the golden table was recorded on (see the module docs).
const BLESSED_PLATFORM: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// Record one case; every case is printed before any assertion fires, so a
/// failing run shows the whole table.
fn check(fails: &mut Vec<String>, label: &str, got: u64, want: u64) {
    println!("{label}: 0x{got:016x}");
    if BLESSED_PLATFORM && got != want {
        fails.push(format!(
            "{label}: p=1 output bits moved (got 0x{got:016x}, golden 0x{want:016x})"
        ));
    }
}

fn finish(fails: Vec<String>) {
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

/// A smooth, non-symmetric complex current density so every DOF is excited.
fn source_fn(x: [f64; 3]) -> [c64; 3] {
    [
        c64::new((1.3 * x[1]).sin(), 0.2 * x[2]),
        c64::new(0.4 * x[0] * x[2], (0.7 * x[0]).cos()),
        c64::new(1.0 + x[0] * x[1], -0.3 * x[1]),
    ]
}

fn cavity() -> (TetMesh, Vec<bool>) {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, mask) = geode_core::assembly::nedelec::cube_pec_interior_edges(&mesh, 1.0);
    (mesh, mask)
}

fn lossy_eps(mesh: &TetMesh) -> Vec<c64> {
    (0..mesh.n_tets())
        .map(|t| c64::new(2.0 + 0.01 * (t % 7) as f64, -0.1))
        .collect()
}

/// Boundary faces of the mesh in the plane `coord[axis] == value`.
fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// Parallel-plate fixture: PEC at y = 0/1 and z = 1, lumped port on z = 0.
fn plate() -> (TetMesh, Vec<bool>, Vec<[u32; 3]>) {
    let mesh = cube_tet_mesh(3, 1.0);
    let walls: Vec<[u32; 3]> = [(1, 0.0), (1, 1.0), (2, 1.0)]
        .iter()
        .flat_map(|&(a, v)| plane_faces(&mesh, a, v))
        .collect();
    let mask = geode_core::mesh::pec_interior_mask_from_triangles(&mesh.edges(), &[&walls]);
    let port_faces = plane_faces(&mesh, 2, 0.0);
    (mesh, mask, port_faces)
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

#[test]
fn p1_single_omega_entry_points_are_bit_identical() {
    let mut fails = Vec::new();
    let (mesh, mask) = cavity();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let eps = lossy_eps(&mesh);
    let src = CurrentSource::from_centroids(&mesh, source_fn);
    let omega = 2.1;

    // Scalar complex ε + σ damping.
    let sigma: Vec<f64> = (0..mesh.n_tets()).map(|t| 0.05 * (t % 3) as f64).collect();
    let sol = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma),
        &bcs,
        omega,
        &src,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "scalar+sigma",
        hash_c(&sol.e_edges),
        0x0424_17fd_9b8a_5738,
    );

    // Diagonal-tensor ε.
    let diag: Vec<[c64; 3]> = (0..mesh.n_tets())
        .map(|t| {
            [
                c64::new(1.5, -0.05),
                c64::new(2.5 + 0.01 * (t % 5) as f64, 0.0),
                c64::new(3.0, -0.2),
            ]
        })
        .collect();
    let sol = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::DiagTensor(&diag),
        None,
        &bcs,
        omega,
        &src,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "diag-tensor",
        hash_c(&sol.e_edges),
        0xda75_13b8_a31d_1d1e,
    );

    // Matched UPML (full symmetric 3×3 ε and ν).
    let lam = [
        [c64::new(1.2, -0.3), c64::new(0.1, 0.02), c64::new(0.0, 0.0)],
        [
            c64::new(0.1, 0.02),
            c64::new(0.9, -0.1),
            c64::new(0.05, 0.0),
        ],
        [c64::new(0.0, 0.0), c64::new(0.05, 0.0), c64::new(1.1, -0.2)],
    ];
    let nu = [
        [c64::new(0.8, 0.2), c64::new(-0.05, 0.0), c64::new(0.0, 0.0)],
        [
            c64::new(-0.05, 0.0),
            c64::new(1.1, 0.1),
            c64::new(0.0, 0.01),
        ],
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.01),
            c64::new(0.95, 0.05),
        ],
    ];
    let eps_t = vec![lam; mesh.n_tets()];
    let nu_t = vec![nu; mesh.n_tets()];
    let sol = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &bcs,
        omega,
        &src,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "matched-upml",
        hash_c(&sol.e_edges),
        0xd5bc_efb0_6dbf_b95a,
    );

    // Degree-2 quadrature source.
    let qsrc = QuadCurrentSource::from_fn(&mesh, |_t, x| source_fn(x));
    let sol = driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &qsrc,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "quad-source",
        hash_c(&sol.e_edges),
        0xdd5e_c6f2_b9bd_8f65,
    );

    // Leontovich good-conductor wall on x = 1 (left free in the mask).
    let wall = plane_faces(&mesh, 0, 1.0);
    let x1_mask = {
        let others: Vec<[u32; 3]> = mesh
            .boundary_faces()
            .into_iter()
            .filter(|f| {
                !f.iter()
                    .all(|&n| (mesh.nodes[n as usize][0] - 1.0).abs() < 1e-12)
            })
            .collect();
        geode_core::mesh::pec_interior_mask_from_triangles(&mesh.edges(), &[&others])
    };
    let sol = driven_solve_with_surface_impedance::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &x1_mask,
        },
        &[SurfaceImpedanceBc {
            triangles: &wall,
            model: SurfaceImpedanceModel::GoodConductor { sigma: 50.0 },
        }],
        omega,
        &src,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "leontovich",
        hash_c(&sol.e_edges),
        0x84df_cfa6_1901_b25c,
    );
    finish(fails);
}

#[test]
fn p1_lumped_port_paths_are_bit_identical() {
    let mut fails = Vec::new();
    let (mesh, mask, port_faces) = plate();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let p = port(&port_faces);

    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&p),
        0.9,
        &zero,
        &device(),
    )
    .unwrap();
    check(
        &mut fails,
        "lumped-port",
        hash_c(&sol.e_edges),
        0xc41b_28ac_91e5_1cf3,
    );

    let omegas: Vec<f64> = (0..9).map(|k| 0.3 + 0.2 * k as f64).collect();
    let dense = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&p),
        &[],
        &omegas,
        &zero,
        &device(),
    )
    .unwrap();
    let z: Vec<c64> = dense.iter().map(|pt| pt.ports[0].z).collect();
    check(
        &mut fails,
        "dense-sweep-z",
        hash_c(&z),
        0x3994_7229_ce65_2177,
    );

    let sp = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&p),
        &[],
        &omegas,
        &device(),
    )
    .unwrap();
    let z: Vec<c64> = sp.iter().flat_map(|pt| pt.z.iter().copied()).collect();
    check(
        &mut fails,
        "s-param-sweep-z",
        hash_c(&z),
        0x3994_7229_ce65_2177,
    );

    let rom = rom_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&p),
        &[],
        &omegas,
        &zero,
        &RomSettings::default(),
        &device(),
    )
    .unwrap();
    let z: Vec<c64> = rom.points.iter().map(|pt| pt.ports[0].z).collect();
    check(
        &mut fails,
        "prom-sweep-z",
        hash_c(&z),
        0xd911_f9d9_4ba6_ce9a,
    );
    finish(fails);
}
