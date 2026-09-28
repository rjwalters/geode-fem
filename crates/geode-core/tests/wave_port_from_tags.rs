//! Tagged port face → [`WavePort`] projection (issue #683, Epic #680
//! Phase 3): [`project_port_face`] / [`wave_port_from_faces`].
//!
//! Every other wave-port test builds its 2-D port mesh in lock-step with
//! the 3-D mesh (`rect_tri_mesh` + node-location lookup). Here the
//! extruded rectangular waveguide section is written out as a Gmsh MSH
//! 4.1 file with **named physical groups**, read back through the generic
//! [`read_tagged_tet_mesh`] path, and the port is built from nothing but
//! the tagged triangles — the path the `geode` CLI takes. Checks:
//!
//! 1. The projected cross-section's TE₁₀ / next cutoffs match the
//!    hand-built `rect_tri_mesh` solve on the identical triangulation to
//!    round-off, and the analytic rectangular cutoffs `k_c = π·√((m/a)² +
//!    (n/b)²)` to the discretization error.
//! 2. The lifted 3-D profile equals the hand-built path's profile (up to
//!    the eigenvector's global sign) and is `S_p`-orthonormal in the 3-D
//!    port-face surface mass.
//! 3. End to end: a straight section with two tag-built ports gives
//!    `|S₁₁| ≈ 0`, `S₂₁ ≈ e^{−jβL}` — the same acceptance as
//!    `tests/wave_port.rs::straight_section_s21_phase_matches_exp_minus_j_beta_l`
//!    — and the same S-matrix as the hand-built ports to round-off.
//! 4. Mixed triangle winding in the tagged list is harmless; non-planar
//!    / empty / zero-amplitude inputs are structured errors, not panics.

use std::f64::consts::PI;
use std::fmt::Write as _;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
use geode_core::assembly::surface::assemble_surface_mass_triplets;
use geode_core::driven::ports::{
    PortFaceError, WavePort, extruded_rect_waveguide_mesh, map_mode_profile_to_full_mesh,
    project_port_face, solve_wave_port_sweep, wave_port_from_faces,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials};
use geode_core::mesh::{TaggedTetMesh, pec_interior_mask_from_triangles, read_tagged_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
const NX: usize = 8;
const NY: usize = 4;
const NZ: usize = 4;

/// Physical tags of the synthetic fixture.
const TAG_GUIDE: i32 = 1;
const TAG_PORT_IN: i32 = 11;
const TAG_PORT_OUT: i32 = 12;
const TAG_WALLS: i32 = 13;

/// Serialize a tet mesh + named surface groups as Gmsh MSH 4.1 ASCII:
/// one volume entity (`guide`) and one surface entity per group.
fn write_msh(
    nodes: &[[f64; 3]],
    tets: &[[u32; 4]],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surfaces.len() + 1);
    for (tag, name, _) in surfaces {
        let _ = writeln!(s, "2 {tag} \"{name}\"");
    }
    let _ = writeln!(s, "3 {TAG_GUIDE} \"guide\"\n$EndPhysicalNames");
    let _ = writeln!(s, "$Entities\n0 0 {} 1", surfaces.len());
    for (i, (tag, _, _)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    let bound: Vec<String> = (1..=surfaces.len()).map(|i| i.to_string()).collect();
    let _ = writeln!(
        s,
        "1 0 0 0 1 1 1 1 {TAG_GUIDE} {} {}\n$EndEntities",
        surfaces.len(),
        bound.join(" ")
    );
    let n = nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_tris: usize = surfaces.iter().map(|(_, _, t)| t.len()).sum();
    let n_el = n_tris + tets.len();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surfaces.len() + 1
    );
    let mut id = 1;
    for (i, (_, _, tris)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, tris.len());
        for t in *tris {
            let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            id += 1;
        }
    }
    let _ = writeln!(s, "3 1 4 {}", tets.len());
    for t in tets {
        let _ = writeln!(
            s,
            "{id} {} {} {} {}",
            t[0] + 1,
            t[1] + 1,
            t[2] + 1,
            t[3] + 1
        );
        id += 1;
    }
    s.push_str("$EndElements\n");
    s
}

/// The extruded `A × B × LEN` section, round-tripped through MSH 4.1 with
/// named groups.
fn tagged_waveguide() -> TaggedTetMesh {
    let g = extruded_rect_waveguide_mesh(NX, NY, NZ, A, B_DIM, LEN);
    let msh = write_msh(
        &g.mesh.nodes,
        &g.mesh.tets,
        &[
            (TAG_PORT_IN, "port_in", &g.port1_faces),
            (TAG_PORT_OUT, "port_out", &g.port2_faces),
            (TAG_WALLS, "walls", &g.sidewall_faces),
        ],
    );
    let tagged = read_tagged_tet_mesh(msh.as_bytes()).expect("synthetic MSH 4.1 parses");
    assert_eq!(tagged.mesh.n_tets(), g.mesh.n_tets());
    assert_eq!(tagged.physical_group_tag(2, "port_in"), Some(TAG_PORT_IN));
    tagged
}

/// The hand-built TE₁₀ profile on the `z = z_plane` port face
/// (`tests/wave_port.rs::build_te10_port`): the `rect_tri_mesh` cross
/// section, its nodes located in the 3-D mesh by coordinates, lifted with
/// [`map_mode_profile_to_full_mesh`] (re-signing any edge whose
/// orientation flips under the relabeling). Returns `(profile, k_c)`.
fn hand_built_te10(tagged: &TaggedTetMesh, z_plane: f64) -> (Vec<f64>, f64) {
    let edges = tagged.mesh.edges();
    let pm = rect_tri_mesh(NX, NY, A, B_DIM);
    let n3 = |x: f64, y: f64| -> u32 {
        tagged
            .mesh
            .nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < 1e-9 && (p[1] - y).abs() < 1e-9 && (p[2] - z_plane).abs() < 1e-9
            })
            .expect("port node") as u32
    };
    let map: Vec<u32> = pm.nodes.iter().map(|p| n3(p[0], p[1])).collect();
    let mut sign_flips = Vec::new();
    let e2d: Vec<[u32; 2]> = pm
        .edges()
        .iter()
        .map(|e| {
            let (a, b) = (map[e[0] as usize], map[e[1] as usize]);
            sign_flips.push(if a < b { 1.0 } else { -1.0 });
            [a.min(b), a.max(b)]
        })
        .collect();
    let hand = &solve_rect_waveguide_modes(&pm, A, B_DIM, 1).expect("hand modal")[0];
    let signed: Vec<f64> = hand
        .e_edges
        .iter()
        .zip(&sign_flips)
        .map(|(v, s)| v * s)
        .collect();
    (
        map_mode_profile_to_full_mesh(&e2d, &signed, &edges),
        hand.k_c,
    )
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

#[test]
fn projected_cutoffs_match_hand_built_and_analytic() {
    let tagged = tagged_waveguide();
    let faces = tagged.triangles_with_tag(TAG_PORT_IN);
    let proj = project_port_face(&tagged.mesh, &faces).expect("planar port face");

    // Plane z = 0: normal ±z, face area a·b.
    assert!(
        (proj.normal[2].abs() - 1.0).abs() < 1e-12,
        "{:?}",
        proj.normal
    );
    assert!(rel(proj.area, A * B_DIM) < 1e-12);
    assert_eq!(proj.tri_mesh.n_nodes(), (NX + 1) * (NY + 1));
    // Rim edges: the rectangle perimeter, 2·(NX + NY) segments.
    let rim = proj.interior_edge_mask.iter().filter(|&&k| !k).count();
    assert_eq!(rim, 2 * (NX + NY));
    // Monotone node map ⇒ global edges keep the lower-first orientation.
    assert!(proj.local_to_global.windows(2).all(|w| w[0] < w[1]));
    assert!(proj.global_edges.iter().all(|e| e[0] < e[1]));

    let n_modes = 3;
    let got = proj.solve_modes(n_modes).expect("modal solve");
    let hand = solve_rect_waveguide_modes(&rect_tri_mesh(NX, NY, A, B_DIM), A, B_DIM, n_modes)
        .expect("hand-built modal solve");
    // Analytic: TE10 = π/2, then the TE20 / TE01 degenerate pair at π.
    let analytic = [PI / A, PI, PI];
    for m in 0..n_modes {
        eprintln!(
            "mode {m}: projected k_c = {:.12}, hand-built = {:.12}, analytic = {:.6}",
            got[m].k_c, hand[m].k_c, analytic[m]
        );
        assert!(
            rel(got[m].k_c, hand[m].k_c) < 1e-9,
            "mode {m}: projected {} vs hand-built {}",
            got[m].k_c,
            hand[m].k_c
        );
        assert!(
            rel(got[m].k_c, analytic[m]) < 0.05,
            "mode {m}: projected {} vs analytic {}",
            got[m].k_c,
            analytic[m]
        );
    }
    // The dominant mode is well resolved on this 8 × 4 cross-section.
    assert!(rel(got[0].k_c, PI / A) < 0.01);
}

#[test]
fn lifted_profile_matches_hand_built_and_is_sp_orthonormal() {
    let tagged = tagged_waveguide();
    let edges = tagged.mesh.edges();
    let faces = tagged.triangles_with_tag(TAG_PORT_IN);
    let port = wave_port_from_faces(&tagged.mesh, &edges, &faces, &[c64::new(1.0, 0.0)])
        .expect("tag-built wave port");
    assert_eq!(port.n_modes(), 1);
    let mode = &port.modes[0].mode;

    let (hand_3d, _) = hand_built_te10(&tagged, 0.0);

    let dot: f64 = mode.iter().zip(&hand_3d).map(|(a, b)| a * b).sum();
    let sign = dot.signum();
    let scale = hand_3d.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let max_diff = mode
        .iter()
        .zip(&hand_3d)
        .map(|(a, b)| (a - sign * b).abs())
        .fold(0.0_f64, f64::max);
    eprintln!("lifted vs hand-built TE10: max |Δ| = {max_diff:.3e} (scale {scale:.3e})");
    assert!(max_diff < 1e-8 * scale, "max |Δ| = {max_diff:e}");

    // S_p-orthonormal in the 3-D port-face tangential surface mass.
    let sp = assemble_surface_mass_triplets(&tagged.mesh, &faces, &edges);
    let norm2: f64 = sp.iter().map(|&(r, c, v)| mode[r] * v * mode[c]).sum();
    assert!((norm2 - 1.0).abs() < 1e-9, "e^T S_p e = {norm2}");
}

#[test]
fn tag_built_straight_section_is_matched_with_exp_minus_j_beta_l() {
    let tagged = tagged_waveguide();
    let edges = tagged.mesh.edges();
    let a_inc = [c64::new(1.0, 0.0)];
    let p_in = wave_port_from_faces(
        &tagged.mesh,
        &edges,
        &tagged.triangles_with_tag(TAG_PORT_IN),
        &a_inc,
    )
    .expect("port_in");
    let p_out = wave_port_from_faces(
        &tagged.mesh,
        &edges,
        &tagged.triangles_with_tag(TAG_PORT_OUT),
        &a_inc,
    )
    .expect("port_out");
    let walls = tagged.triangles_with_tag(TAG_WALLS);
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let eps = vec![c64::new(1.0, 0.0); tagged.mesh.n_tets()];

    // Same operating point as tests/wave_port.rs (TE10 propagating,
    // k_c = π/2 < ω).
    let omega = 2.5;
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let run = |ports: &[WavePort]| {
        solve_wave_port_sweep::<B>(
            &tagged.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            ports,
            &[omega],
            &device,
        )
        .expect("wave-port sweep")
        .remove(0)
    };
    let pt = run(&[p_in, p_out]);
    let beta = pt.beta[0].re;
    let s11 = pt.s[0];
    let s21 = pt.s[2];
    let want = c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
    eprintln!(
        "tag-built straight section: |S11| = {:.3e}, S21 = {s21}, e^(-jβL) = {want}, \
         |S12 - S21| = {:.3e}",
        s11.norm(),
        (pt.s[1] - s21).norm()
    );
    // tests/wave_port.rs straight-section acceptance, verbatim.
    assert!(s11.norm() < 0.5, "|S11| = {}", s11.norm());
    assert!((s21.norm() - 1.0).abs() < 0.5, "|S21| = {}", s21.norm());
    assert!((pt.s[1] - s21).norm() / s21.norm() < 0.1, "reciprocity");
    assert!((s21 - want).norm() < 0.1, "S21 = {s21} vs {want}");

    // Parity with the hand-built ports on the same mesh: the S-matrix
    // agrees to round-off (up to each port mode's free global sign, which
    // the S-matrix is invariant to on the diagonal and which flips the
    // off-diagonal consistently — compare magnitudes and S11 exactly).
    let hand_ports: Vec<WavePort> = [(0.0, TAG_PORT_IN), (LEN, TAG_PORT_OUT)]
        .iter()
        .map(|&(z, tag)| {
            let (mode, k_c) = hand_built_te10(&tagged, z);
            WavePort::single_mode(
                tagged.triangles_with_tag(tag),
                mode,
                k_c,
                c64::new(1.0, 0.0),
            )
        })
        .collect();
    let hand = run(&hand_ports);
    for k in 0..4 {
        let d = (pt.s[k].norm() - hand.s[k].norm()).abs();
        assert!(
            d < 1e-8,
            "|S[{k}]| tag-built {} vs hand-built {}",
            pt.s[k],
            hand.s[k]
        );
    }
    assert!((pt.s[0] - hand.s[0]).norm() < 1e-8);
    assert!((pt.s[3] - hand.s[3]).norm() < 1e-8);
}

#[test]
fn mixed_winding_is_harmless_and_bad_faces_are_structured_errors() {
    let tagged = tagged_waveguide();
    let faces = tagged.triangles_with_tag(TAG_PORT_IN);
    let base = project_port_face(&tagged.mesh, &faces).unwrap();
    let k_base = base.solve_modes(1).unwrap()[0].k_c;

    // Flip the winding of every other triangle: same plane, same modes.
    let mixed: Vec<[u32; 3]> = faces
        .iter()
        .enumerate()
        .map(|(i, t)| if i % 2 == 0 { *t } else { [t[0], t[2], t[1]] })
        .collect();
    let proj = project_port_face(&tagged.mesh, &mixed).unwrap();
    let k_mixed = proj.solve_modes(1).unwrap()[0].k_c;
    assert!(rel(k_mixed, k_base) < 1e-12, "{k_mixed} vs {k_base}");

    // Port face + one sidewall triangle: not planar.
    let mut bent = faces.clone();
    bent.push(tagged.triangles_with_tag(TAG_WALLS)[0]);
    assert!(matches!(
        project_port_face(&tagged.mesh, &bent),
        Err(PortFaceError::NonPlanar { .. })
    ));

    // Empty face list, bad amplitudes.
    assert!(matches!(
        project_port_face(&tagged.mesh, &[]),
        Err(PortFaceError::Empty)
    ));
    let edges = tagged.mesh.edges();
    assert!(matches!(
        base.wave_port(&edges, &[]),
        Err(PortFaceError::InvalidAmplitude(_))
    ));
    assert!(matches!(
        base.wave_port(&edges, &[c64::new(0.0, 0.0)]),
        Err(PortFaceError::InvalidAmplitude(_))
    ));
    // 8 × 4 cross-section: 84 interior edges − 21 interior nodes = 63
    // physical modes at most; more is a structured error, not a panic.
    assert_eq!(base.n_interior_nodes(), 21);
    assert!(matches!(
        base.solve_modes(64),
        Err(PortFaceError::TooFewModes {
            requested: 64,
            found: 63
        })
    ));
    // A single triangle has no interior edge.
    assert!(matches!(
        project_port_face(&tagged.mesh, &faces[..1]),
        Err(PortFaceError::NoInteriorEdges)
    ));
}
