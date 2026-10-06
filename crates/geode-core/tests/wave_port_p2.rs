//! Scratch measurement: 3-D p=2 TM110 undershoot on the #824 box harness.

use burn::tensor::backend::BackendTypes;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use geode_core::eigen::pec_cavity::{
    PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes_on_space,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn box_lowest_k(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> f64 {
    let space = HcurlSpace::build(mesh, order);
    let walls = mesh.boundary_faces();
    let mask = space.pec_interior_mask(mesh, &[&walls]).expect("mask");
    let eps = vec![1.0; mesh.n_tets()];
    let tm11 = ((std::f64::consts::PI / a).powi(2) + (std::f64::consts::PI / b).powi(2)).sqrt();
    let settings = PecCavitySettings::new((0.75 * tm11).powi(2), 2);
    let modes = solve_pec_cavity_modes_on_space::<B>(
        &space,
        mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &mask,
        &[],
        &settings,
        &device(),
    )
    .expect("box eigensolve");
    modes.modes.modes[0].k0
}

#[test]
#[ignore = "measurement"]
fn measure_p2_tm_undershoot() {
    let cases: &[(f64, f64, usize, f64)] = &[
        (2.0, 1.0, 1, 0.75),
        (2.0, 1.0, 1, 0.6),
        (2.0, 1.0, 1, 0.5),
        (2.0, 1.0, 2, 0.75),
        (2.0, 1.0, 2, 0.5),
        (2.0, 1.0, 4, 0.5),
        (4.0, 2.0, 1, 1.5),
        (4.0, 2.0, 1, 1.95),
    ];
    for &(a, b, nz, length) in cases {
        let tm11 = ((std::f64::consts::PI / a).powi(2) + (std::f64::consts::PI / b).powi(2)).sqrt();
        let kh = tm11 * length / nz as f64;
        for n in [2usize, 4, 8] {
            let g = extruded_rect_waveguide_mesh(2 * n, n, nz, a, b, length);
            let k2 = box_lowest_k(&g.mesh, ElementOrder::P2, a, b);
            let u2 = 1.0 - k2 / tm11;
            eprintln!(
                "MEAS {a}x{b} face {}x{n} nz {nz} h {:.4} kh {kh:.3}: p2 {k2:.5} ({:+.3} %) \
                 /kh2 {:.5} /kh4 {:.6}",
                2 * n,
                length / nz as f64,
                -100.0 * u2,
                u2 / (kh * kh),
                u2 / kh.powi(4)
            );
        }
    }
}

fn tm_like_k(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Option<f64> {
    use faer::c64;
    let space = HcurlSpace::build(mesh, order);
    let walls = mesh.boundary_faces();
    let mask = space.pec_interior_mask(mesh, &[&walls]).expect("mask");
    let eps = vec![1.0; mesh.n_tets()];
    let tm11 = ((std::f64::consts::PI / a).powi(2) + (std::f64::consts::PI / b).powi(2)).sqrt();
    let mut n_modes = 24;
    let modes = loop {
        let mut settings = PecCavitySettings::new((0.45 * tm11).powi(2), n_modes);
        settings.max_iters = 480;
        match solve_pec_cavity_modes_on_space::<B>(
            &space,
            mesh,
            &PecCavityMaterials::Isotropic(&eps),
            &mask,
            &[],
            &settings,
            &device(),
        ) {
            Ok(m) => break m,
            Err(geode_core::eigen::pec_cavity::PecCavityError::TooFewModes { found, .. })
                if found > 0 && found < n_modes =>
            {
                n_modes = found
            }
            Err(e) => panic!("box eigensolve: {e:?}"),
        }
    };
    let kept: Vec<usize> = (0..space.n_dofs()).filter(|&d| mask[d]).collect();
    modes
        .modes
        .modes
        .iter()
        .filter(|m| {
            let mut x = vec![c64::new(0.0, 0.0); space.n_dofs()];
            for (i, &d) in kept.iter().enumerate() {
                x[d] = c64::new(m.vector[i], 0.0);
            }
            let (mut ez, mut all) = (0.0, 0.0);
            for t in 0..mesh.n_tets() {
                for bary in [
                    [0.25, 0.25, 0.25, 0.25],
                    [0.55, 0.15, 0.15, 0.15],
                    [0.15, 0.55, 0.15, 0.15],
                    [0.15, 0.15, 0.55, 0.15],
                    [0.15, 0.15, 0.15, 0.55],
                ] {
                    let e = space.field_at(mesh, t, bary, &x);
                    ez += e[2].norm_sqr();
                    all += e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr();
                }
            }
            if std::env::var("P2_DBG").is_ok() {
                eprintln!("  dbg {order:?} k {:.4} ez {:.3}", m.k0, ez / all);
            }
            ez > 0.5 * all
        })
        .map(|m| m.k0)
        .reduce(f64::min)
}

fn measure_one(label: &str, mesh: &TetMesh, a: f64, b: f64) {
    use geode_core::driven::ports::{project_port_face, tm_guard_axial_reach};
    let tm11 = ((std::f64::consts::PI / a).powi(2) + (std::f64::consts::PI / b).powi(2)).sqrt();
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-9))
        .collect();
    let face = project_port_face(mesh, &port).expect("port face");
    let est = face.tm_cutoff_estimate(None).expect("est");
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    let h = face.guide_axial_spacing(mesh, reach);
    let kh = est.k_c() * h;
    let k1 = tm_like_k(mesh, ElementOrder::P1, a, b).unwrap_or(f64::NAN);
    let k2 = tm_like_k(mesh, ElementOrder::P2, a, b).expect("p2 TM-like");
    let u1 = 1.0 - k1 / est.k_c();
    let u2 = 1.0 - k2 / est.k_c();
    eprintln!(
        "MEAS2 {label}: k_c_est {:.4} (an {tm11:.4}) h_n {h:.3} kh {kh:.3} | p1 {k1:.4} u {:+.3}% /kh2 {:.5} | p2 {k2:.4} u {:+.3}% /kh2 {:.6} /kh4 {:.7}",
        est.k_c(),
        100.0 * u1,
        u1 / (kh * kh),
        100.0 * u2,
        u2 / (kh * kh),
        u2 / kh.powi(4)
    );
}

#[test]
#[ignore = "measurement"]
fn measure_p2_tm_undershoot_v2() {
    // Structured, larger kh (one layer), TM-like classifier.
    for &(a, b, nz, length) in &[
        (2.0, 1.0, 1usize, 1.0),
        (2.0, 1.0, 1, 1.25),
        (2.0, 1.0, 1, 1.5),
        (2.0, 1.0, 2, 3.0),
        (4.0, 2.0, 1, 2.5),
        (4.0, 2.0, 1, 3.0),
    ] {
        for n in [2usize, 4, 8] {
            let g = extruded_rect_waveguide_mesh(2 * n, n, nz, a, b, length);
            measure_one(&format!("struct {a}x{b} face {}x{n} nz {nz} L {length}", 2 * n), &g.mesh, a, b);
        }
    }
    // Stepped layers (fine at the port, coarse behind).
    for (nx, ny, a, b, zs) in [
        (16usize, 8usize, 2.0, 1.0, [0.0, 0.15, 0.75]),
        (24, 8, 3.0, 1.0, [0.0, 0.2, 0.9]),
        (8, 4, 2.0, 1.0, [0.0, 0.15, 1.15]),
        (4, 2, 2.0, 1.0, [0.0, 0.25, 1.25]),
    ] {
        let mut g = extruded_rect_waveguide_mesh(nx, ny, 2, a, b, 1.0);
        for p in &mut g.mesh.nodes {
            p[2] = zs[(2.0 * p[2]).round() as usize];
        }
        measure_one(&format!("stepped {a}x{b} face {nx}x{ny} zs {zs:?}"), &g.mesh, a, b);
    }
    // Gmsh unstructured.
    if let Ok(dir) = std::env::var("P2_MSH_DIR") {
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "msh"))
            .collect();
        files.sort();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            let (a, b) = if name.starts_with("g_4x2") {
                (4.0, 2.0)
            } else if name.starts_with("g_1x1") {
                (1.0, 1.0)
            } else {
                (2.0, 1.0)
            };
            let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&f).unwrap())
                .expect("msh");
            measure_one(&format!("gmsh {name}"), &tagged.mesh, a, b);
        }
    }
}
