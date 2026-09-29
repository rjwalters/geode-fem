//! Open-path current excitation (issue #714): the P1-conduction-derived
//! current density `J = −σ∇φ` for an arbitrary conductor + source/sink
//! faces, validated against the analytic builders of
//! `assembly::magnetostatic3d` and against the magnetostatic solver's
//! discrete-solenoidality compatibility gate.
//!
//! * Straight wire (solid coax core, end-cap terminals) vs
//!   `axial_current_density`: `J` agrees per tet to round-off, the
//!   compatibility check passes at round-off, and the extracted `L`
//!   matches the analytic-excitation `L` to round-off.
//! * Open ring (a toroidal tube with one azimuthal sector removed as the
//!   cut; terminals on the two cut faces) vs `loop_current_density` away
//!   from the cut. A closed loop has no source/sink pair — see the
//!   `current_path` module docs; the cut ring is the open-path analogue.
//! * Tripwires: a perturbed `J` fails the compatibility check; the ring's
//!   terminal nodes (not on PEC) fail it when not exempted and are reported
//!   by `ungrounded_nodes`; malformed paths are rejected.
//! * Per-component balance: with the PEC wall reduced to two disconnected
//!   end caps (no shield), the grounded node set splits in two, the source
//!   and sink land on different components, and `check_grounded_balance`
//!   rejects the path (the PR #718 judge repro).
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;

use geode_core::assembly::current_path::{
    CurrentPathError, check_grounded_balance, component_net_currents, grounded_components,
    open_path_current, triangle_node_components, ungrounded_nodes,
};
use geode_core::assembly::magnetostatic3d::{
    CurrentTerminal, assemble_current_rhs, assemble_magnetostatic3d, axial_current_density,
    check_solenoidal, extract_inductance, loop_current_density, measure_axial_current,
    measure_loop_current,
};
use geode_core::assembly::nedelec::pec_interior_edge_mask;
use geode_core::mesh::TetMesh;
use geode_core::mesh::magnetostatic_fixtures::{
    cylinder_cap_nodes, cylinder_pec_interior_mask, loop_pair_mesh, solid_coax_mesh,
};
use geode_core::mesh::pec_interior_mask_from_triangles;

const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0; 3];
    for &v in &mesh.tets[t] {
        for d in 0..3 {
            c[d] += 0.25 * mesh.nodes[v as usize][d];
        }
    }
    c
}

/// Faces of `conductor` tets whose three nodes all satisfy `on`.
fn conductor_faces(
    mesh: &TetMesh,
    conductor: &[bool],
    on: impl Fn([f64; 3]) -> bool,
) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    for (t, tet) in mesh.tets.iter().enumerate() {
        if !conductor[t] {
            continue;
        }
        for f in FACES {
            let tri = [tet[f[0]], tet[f[1]], tet[f[2]]];
            if tri.iter().all(|&v| on(mesh.nodes[v as usize])) {
                out.push(tri);
            }
        }
    }
    out
}

struct Coax {
    mesh: TetMesh,
    conductor: Vec<bool>,
    source: Vec<[u32; 3]>,
    sink: Vec<[u32; 3]>,
    mask: Vec<bool>,
}

fn coax(a: f64, b: f64, length: f64, nth: usize, nr: usize, nz: usize) -> Coax {
    let fx = solid_coax_mesh(a, b, length, nth, nr, nz);
    let mesh = fx.mesh;
    let conductor: Vec<bool> = (0..mesh.n_tets())
        .map(|t| {
            let c = centroid(&mesh, t);
            (c[0] * c[0] + c[1] * c[1]).sqrt() <= a
        })
        .collect();
    let tol = 1e-9;
    let source = conductor_faces(&mesh, &conductor, |p| p[2].abs() < tol);
    let sink = conductor_faces(&mesh, &conductor, |p| (p[2] - length).abs() < tol);
    let (_e, mask) = cylinder_pec_interior_mask(&mesh, b, length);
    Coax {
        mesh,
        conductor,
        source,
        sink,
        mask,
    }
}

#[test]
fn straight_wire_matches_axial_builder_and_is_solenoidal() {
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 24, 8, 3);
    let sigma = vec![1.0; cx.mesh.n_tets()];
    let path = open_path_current(
        &cx.mesh,
        "wire",
        &cx.conductor,
        &sigma,
        &cx.source,
        &cx.sink,
    )
    .expect("open path");
    assert_eq!(path.terminal.current, 1.0);
    // Discrete conservation: sink current = source current (round-off).
    assert!(
        (path.sink_current - 1.0).abs() < 1e-10,
        "sink Galerkin current {} != 1",
        path.sink_current
    );
    // Uniform J ⇒ geometric face fluxes agree exactly with the Galerkin 1 A.
    assert!((path.source_face_flux - 1.0).abs() < 1e-10);
    assert!((path.sink_face_flux - 1.0).abs() < 1e-10);

    // Oracle: the analytic axial builder, renormalised to its measured 1 A.
    let j_ax = axial_current_density(&cx.mesh, a, 1.0);
    let i_ax = measure_axial_current(&cx.mesh, &j_ax, length);
    let jz_ref = 1.0 / (PI * a * a) / i_ax;
    let mut worst = 0.0_f64;
    for t in 0..cx.mesh.n_tets() {
        let want = [0.0, 0.0, j_ax[t][2] / i_ax];
        for d in 0..3 {
            worst = worst.max((path.terminal.j[t][d] - want[d]).abs() / jz_ref);
        }
    }
    assert!(worst < 1e-10, "J vs axial builder max rel diff {worst:e}");

    // Compatibility gate at round-off (not just the 1e-6 production tol).
    let mu_r = vec![1.0; cx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&cx.mesh, &mu_r, &cx.mask).unwrap();
    let rhs = assemble_current_rhs(&sys, &cx.mesh, &path.terminal.j).unwrap();
    let res = check_solenoidal(&sys, &rhs, &path.terminal.exempt_nodes, 1e-12)
        .expect("conduction-derived J must be discretely solenoidal");
    eprintln!("straight wire: max rel discrete-divergence residual {res:e}");
    // Terminals are on the PEC end caps.
    assert!(ungrounded_nodes(&sys, &path.terminal.exempt_nodes).is_empty());
}

#[test]
fn straight_wire_inductance_matches_axial_excitation() {
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 32, 12, 2);
    let sigma = vec![1.0; cx.mesh.n_tets()];
    let path = open_path_current(
        &cx.mesh,
        "wire",
        &cx.conductor,
        &sigma,
        &cx.source,
        &cx.sink,
    )
    .unwrap();
    let mu_r = vec![1.0; cx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&cx.mesh, &mu_r, &cx.mask).unwrap();

    let j_ax = axial_current_density(&cx.mesh, a, 1.0);
    let i_ax = measure_axial_current(&cx.mesh, &j_ax, length);
    let analytic = CurrentTerminal {
        name: "axial".into(),
        j: j_ax,
        current: i_ax,
        exempt_nodes: cylinder_cap_nodes(&cx.mesh, length),
    };
    let lm = extract_inductance(&sys, &cx.mesh, &[path.terminal, analytic], 1e-6).unwrap();
    let rel = (lm.l[0][0] - lm.l[1][1]).abs() / lm.l[1][1];
    assert!(
        rel < 1e-9,
        "L(conduction) {} vs L(axial) {}",
        lm.l[0][0],
        lm.l[1][1]
    );
    // Same current distribution ⇒ mutual = self (coupling coefficient 1).
    assert!((lm.l[0][1] - lm.l[1][1]).abs() / lm.l[1][1] < 1e-9);
    assert!((lm.flux_linkage_diag[0] - lm.l[0][0]).abs() / lm.l[0][0] < 1e-8);
}

#[test]
fn perturbed_current_fails_the_compatibility_check() {
    // Sensitivity of the gate: scale J in one interior conductor tet by 10%
    // and the (otherwise round-off) residual must trip a 1e-6 tolerance.
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 24, 8, 3);
    let sigma = vec![1.0; cx.mesh.n_tets()];
    let mut path = open_path_current(
        &cx.mesh,
        "wire",
        &cx.conductor,
        &sigma,
        &cx.source,
        &cx.sink,
    )
    .unwrap();
    let t = (0..cx.mesh.n_tets())
        .find(|&t| {
            let c = centroid(&cx.mesh, t);
            cx.conductor[t] && (c[2] - 0.5 * length).abs() < 0.2 && c[0].hypot(c[1]) < 0.5 * a
        })
        .unwrap();
    for d in 0..3 {
        path.terminal.j[t][d] *= 1.1;
    }
    let mu_r = vec![1.0; cx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&cx.mesh, &mu_r, &cx.mask).unwrap();
    let rhs = assemble_current_rhs(&sys, &cx.mesh, &path.terminal.j).unwrap();
    assert!(check_solenoidal(&sys, &rhs, &path.terminal.exempt_nodes, 1e-6).is_err());
}

struct Ring {
    mesh: TetMesh,
    conductor: Vec<bool>,
    source: Vec<[u32; 3]>,
    sink: Vec<[u32; 3]>,
    mask: Vec<bool>,
    r_loop: f64,
    z_loop: f64,
    r_tube: f64,
    n_theta: usize,
}

/// A toroidal tube (the `loop_current_density` tet selection) with the
/// azimuthal sector `θ ∈ [0, 2π/n_θ]` removed as the cut. Source = the cut
/// face at `θ = 2π/n_θ`, sink = the cut face at `θ = 0`, so current flows
/// in `+θ̂` through the remaining `n_θ − 1` sectors.
fn cut_ring() -> Ring {
    let (r_loop, z_loop, r_tube) = (1.0, 1.0, 0.3);
    let (r_box, length) = (2.0, 2.0);
    let n_theta = 48;
    let fx = loop_pair_mesh(
        r_loop, z_loop, r_loop, z_loop, r_box, length, n_theta, 16, 16,
    );
    let mesh = fx.mesh;
    let dth = 2.0 * PI / n_theta as f64;
    let angle = |p: [f64; 3]| {
        let a = p[1].atan2(p[0]);
        if a < 0.0 { a + 2.0 * PI } else { a }
    };
    let conductor: Vec<bool> = (0..mesh.n_tets())
        .map(|t| {
            let c = centroid(&mesh, t);
            let rho = c[0].hypot(c[1]);
            let in_tube = (rho - r_loop).hypot(c[2] - z_loop) <= r_tube && rho > 1e-12;
            in_tube && angle(c) > dth
        })
        .collect();
    let ang_tol = 1e-9;
    let on_angle = |target: f64| {
        move |p: [f64; 3]| {
            let rho = p[0].hypot(p[1]);
            if rho < 1e-12 {
                return false;
            }
            let a = angle(p);
            (a - target).abs() < ang_tol || (a - target - 2.0 * PI).abs() < ang_tol
        }
    };
    let source = conductor_faces(&mesh, &conductor, on_angle(dth));
    let sink = conductor_faces(&mesh, &conductor, on_angle(0.0));
    let tolb = 1e-6 * r_box;
    let tolz = 1e-6 * length;
    let on_bdry: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| {
            (p[0].hypot(p[1]) - r_box).abs() < tolb
                || p[2].abs() < tolz
                || (p[2] - length).abs() < tolz
        })
        .collect();
    let mask = pec_interior_edge_mask(&mesh.edges(), &on_bdry);
    Ring {
        mesh,
        conductor,
        source,
        sink,
        mask,
        r_loop,
        z_loop,
        r_tube,
        n_theta,
    }
}

#[test]
fn open_ring_matches_loop_builder_away_from_the_cut() {
    let ring = cut_ring();
    assert!(!ring.source.is_empty() && !ring.sink.is_empty());
    let sigma = vec![1.0; ring.mesh.n_tets()];
    let path = open_path_current(
        &ring.mesh,
        "ring",
        &ring.conductor,
        &sigma,
        &ring.source,
        &ring.sink,
    )
    .expect("open ring path");
    assert!(
        (path.sink_current - 1.0).abs() < 1e-10,
        "Galerkin sink current {} (conservation)",
        path.sink_current
    );
    // Independent current measures vs the 1 A Galerkin normalisation:
    // the volume-averaged azimuthal current (`measure_loop_current` over
    // the n_θ − 1 conducting sectors) and the geometric face fluxes.
    let frac = (ring.n_theta - 1) as f64 / ring.n_theta as f64;
    let i_vol = measure_loop_current(&ring.mesh, &path.terminal.j) / frac;
    eprintln!(
        "open ring: volume-avg current {i_vol:.6}, source face flux {:.6}, sink face flux {:.6}, \
         sink Galerkin - 1 = {:.3e}",
        path.source_face_flux,
        path.sink_face_flux,
        path.sink_current - 1.0
    );
    assert!((i_vol - 1.0).abs() < 0.02, "volume-avg current {i_vol}");
    for (what, f) in [
        ("source", path.source_face_flux),
        ("sink", path.sink_face_flux),
    ] {
        assert!(
            (f - 1.0).abs() < 0.10,
            "{what} face flux {f} vs 1 A Galerkin current"
        );
    }

    // Oracles on the same tube tets, each renormalised to 1 A:
    //  (1) `loop_current_density` itself (uniform |J| = J₀ θ̂), and
    //  (2) the exact open-ring conduction field J = (C/ρ) θ̂ — the
    //      builder's direction field modulated by R/ρ (Laplace on a
    //      toroidal wedge with φ ∝ θ gives |∇φ| ∝ 1/ρ).
    let j_loop = loop_current_density(&ring.mesh, ring.r_loop, ring.z_loop, ring.r_tube, 1.0);
    let i_loop = measure_loop_current(&ring.mesh, &j_loop);
    let j_exact: Vec<[f64; 3]> = (0..ring.mesh.n_tets())
        .map(|t| {
            let c = centroid(&ring.mesh, t);
            let w = ring.r_loop / c[0].hypot(c[1]).max(1e-12);
            [j_loop[t][0] * w, j_loop[t][1] * w, j_loop[t][2] * w]
        })
        .collect();
    let i_exact = measure_loop_current(&ring.mesh, &j_exact);
    let dth = 2.0 * PI / ring.n_theta as f64;
    let compare = |oracle: &[[f64; 3]], scale: f64| {
        let (mut num, mut den, mut cos_min) = (0.0_f64, 0.0_f64, 1.0_f64);
        let mut n = 0;
        for t in 0..ring.mesh.n_tets() {
            if !ring.conductor[t] {
                continue;
            }
            let c = centroid(&ring.mesh, t);
            let a = c[1].atan2(c[0]).rem_euclid(2.0 * PI);
            // Away from the cut: at least 4 sectors from either cut face.
            if a < 5.0 * dth || a > 2.0 * PI - 4.0 * dth {
                continue;
            }
            let want = oracle[t].map(|x| x / scale);
            let got = path.terminal.j[t];
            let (mut d2, mut w2, mut gw, mut g2) = (0.0, 0.0, 0.0, 0.0);
            for d in 0..3 {
                d2 += (got[d] - want[d]).powi(2);
                w2 += want[d] * want[d];
                gw += got[d] * want[d];
                g2 += got[d] * got[d];
            }
            num += d2;
            den += w2;
            cos_min = cos_min.min(gw / (g2.sqrt() * w2.sqrt()));
            n += 1;
        }
        ((num / den).sqrt(), cos_min, n)
    };
    let (rel_builder, cos_min, n) = compare(&j_loop, i_loop);
    let (rel_exact, _, _) = compare(&j_exact, i_exact);
    eprintln!(
        "open ring over {n} tets away from the cut: rel L2 vs loop builder {rel_builder:.4}, \
         vs exact C/rho field {rel_exact:.4}, min direction cosine {cos_min:.6}"
    );
    assert!(n > 100);
    // Direction: the conduction J is azimuthal away from the cut.
    assert!(cos_min > 0.99, "min cos(J, θ̂) = {cos_min}");
    // Magnitude vs the exact open-ring conduction field: the residual is
    // the O(h) error of a piecewise-constant gradient on this coarse tube
    // (h ≈ 0.13 R; measured 6.5%) plus the jagged centroid-selected
    // cross-section.
    assert!(
        rel_exact < 0.10,
        "rel L2 diff vs exact C/ρ field {rel_exact}"
    );
    // vs the uniform-|J| builder: the ±r_tube/R = ±30% 1/ρ profile is the
    // whole (expected, physical) difference.
    assert!(
        rel_builder < 0.20,
        "rel L2 diff vs loop builder {rel_builder}"
    );

    // Compatibility gate with the terminal nodes exempt: round-off.
    let mu_r = vec![1.0; ring.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&ring.mesh, &mu_r, &ring.mask).unwrap();
    let rhs = assemble_current_rhs(&sys, &ring.mesh, &path.terminal.j).unwrap();
    let res = check_solenoidal(&sys, &rhs, &path.terminal.exempt_nodes, 1e-12)
        .expect("conduction-derived ring J must be discretely solenoidal off the terminals");
    eprintln!("open ring: max rel discrete-divergence residual {res:e}");
    // …but the cut faces float in the interior (not on PEC): the charge
    // piles up there, the check trips without the exemption, and the
    // grounding helper flags every terminal node. This is why v1 requires
    // terminals on the PEC wall.
    assert!(check_solenoidal(&sys, &rhs, &[], 1e-6).is_err());
    let floating = ungrounded_nodes(&sys, &path.terminal.exempt_nodes);
    assert_eq!(floating.len(), path.terminal.exempt_nodes.len());
}

#[test]
fn malformed_paths_are_rejected() {
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 12, 4, 2);
    let n = cx.mesh.n_tets();
    let sigma = vec![1.0; n];
    let m = &cx.mesh;

    // Source == sink (overlapping terminals).
    let e = open_path_current(m, "p", &cx.conductor, &sigma, &cx.source, &cx.source).unwrap_err();
    assert!(matches!(e, CurrentPathError::TerminalsOverlap(_)), "{e}");
    // Empty terminal.
    let e = open_path_current(m, "p", &cx.conductor, &sigma, &[], &cx.sink).unwrap_err();
    assert!(matches!(e, CurrentPathError::EmptyTerminal(_)), "{e}");
    // Empty conductor.
    let none = vec![false; n];
    let e = open_path_current(m, "p", &none, &sigma, &cx.source, &cx.sink).unwrap_err();
    assert!(matches!(e, CurrentPathError::EmptyConductor(_)), "{e}");
    // A terminal on the dielectric (outer cap annulus), not the conductor.
    let dielectric: Vec<bool> = cx.conductor.iter().map(|c| !c).collect();
    let annulus = conductor_faces(m, &dielectric, |p| p[2].abs() < 1e-9);
    let e = open_path_current(m, "p", &cx.conductor, &sigma, &annulus, &cx.sink).unwrap_err();
    assert!(
        matches!(e, CurrentPathError::TerminalNotOnConductor(_)),
        "{e}"
    );
    // Non-positive conductivity on the conductor.
    let mut bad = sigma.clone();
    let t = cx.conductor.iter().position(|&c| c).unwrap();
    bad[t] = 0.0;
    let e = open_path_current(m, "p", &cx.conductor, &bad, &cx.source, &cx.sink).unwrap_err();
    assert!(matches!(e, CurrentPathError::BadConductivity(_)), "{e}");
    // Length mismatch.
    let e =
        open_path_current(m, "p", &cx.conductor[1..], &sigma, &cx.source, &cx.sink).unwrap_err();
    assert!(matches!(e, CurrentPathError::ShapeMismatch(_)), "{e}");
}

#[test]
fn disconnected_source_and_sink_are_rejected() {
    // Conductor = two slabs (z < 0.4 and z > 0.6) with nothing in between:
    // the source slab floats at 1 V, the sink slab at 0 V, no current.
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 12, 4, 5);
    let split: Vec<bool> = (0..cx.mesh.n_tets())
        .map(|t| {
            let z = centroid(&cx.mesh, t)[2];
            cx.conductor[t] && !(0.4..0.6).contains(&z)
        })
        .collect();
    let sigma = vec![1.0; cx.mesh.n_tets()];
    let e = open_path_current(&cx.mesh, "p", &split, &sigma, &cx.source, &cx.sink).unwrap_err();
    assert!(
        matches!(
            e,
            CurrentPathError::NoCurrent(_) | CurrentPathError::Conduction(_)
        ),
        "{e}"
    );
}

#[test]
fn source_and_sink_on_disconnected_pec_components_are_rejected() {
    let (a, b, length) = (1.0, 3.0, 1.0);
    let cx = coax(a, b, length, 16, 6, 3);
    let sigma = vec![1.0; cx.mesh.n_tets()];
    let mu_r = vec![1.0; cx.mesh.n_tets()];
    let path = open_path_current(
        &cx.mesh,
        "wire",
        &cx.conductor,
        &sigma,
        &cx.source,
        &cx.sink,
    )
    .unwrap();

    // Shielded coax: caps + outer wall form ONE grounded component; the
    // path balances on it to round-off.
    let sys = assemble_magnetostatic3d(&cx.mesh, &mu_r, &cx.mask).unwrap();
    let comps = grounded_components(&sys);
    assert_eq!(comps.count, 1, "caps + shield must be one PEC component");
    let worst = check_grounded_balance(&sys, &cx.mesh, &path.terminal, 1e-9)
        .expect("same-component path balances");
    assert!(worst < 1e-9, "shielded imbalance {worst:e}");

    // Shield dropped: only the two end-cap disks are PEC.
    let everywhere = vec![true; cx.mesh.n_tets()];
    let tol = 1e-9;
    let cap0 = conductor_faces(&cx.mesh, &everywhere, |p| p[2].abs() < tol);
    let cap1 = conductor_faces(&cx.mesh, &everywhere, |p| (p[2] - length).abs() < tol);
    let edges = cx.mesh.edges();
    let caps_mask = pec_interior_mask_from_triangles(&edges, &[&cap0, &cap1]);
    let sys_caps = assemble_magnetostatic3d(&cx.mesh, &mu_r, &caps_mask).unwrap();
    let comps = grounded_components(&sys_caps);
    assert_eq!(comps.count, 2, "two disconnected end caps");
    // Still every terminal node is "grounded" — the old, insufficient check.
    assert!(ungrounded_nodes(&sys_caps, &path.terminal.exempt_nodes).is_empty());
    // The triangle-based components (the CLI's pre-solve view) agree.
    let tri = triangle_node_components(cx.mesh.n_nodes(), &[&cap0, &cap1]);
    assert_eq!(tri, comps);
    let src_c = comps.touched(&path.source_nodes);
    let sink_c = comps.touched(&path.sink_nodes);
    assert_eq!(src_c.len(), 1);
    assert_eq!(sink_c.len(), 1);
    assert_ne!(src_c, sink_c);
    // Net current per component: +1 A leaves the source cap, 1 A returns
    // into the sink cap.
    let net = component_net_currents(&cx.mesh, &path.terminal.j, &comps);
    assert!((net[src_c[0] as usize] - 1.0).abs() < 1e-9, "{net:?}");
    assert!((net[sink_c[0] as usize] + 1.0).abs() < 1e-9, "{net:?}");
    match check_grounded_balance(&sys_caps, &cx.mesh, &path.terminal, 1e-6) {
        Err(CurrentPathError::UnbalancedGround(msg)) => {
            assert!(msg.contains("same connected PEC component"), "{msg}")
        }
        other => panic!("expected UnbalancedGround, got {other:?}"),
    }
}
