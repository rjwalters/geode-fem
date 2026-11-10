//! Far-field (NTFF) figure-of-merit shape gradient on the **curved conformal**
//! open radiator (issue #652, Epic #647 Phase 5, v1.5 stretch).
//!
//! Gates, all on the curved smoke fixture (`read_patch_smoke_curved_fixture`,
//! matched box-UPML + lossy FR-4 + pinned-feed lumped port):
//!
//! 1. the NTFF runs on the curved driven solution and is physical (positive
//!    directivity, radiated power ≤ accepted power, passive `|S₁₁| ≤ 1`),
//! 2. the edge-space covectors reproduce the grid far field and the radiation
//!    intensity integrates to the radiated power (normalisation check),
//! 3. the realized-gain shape gradient (one forward + one adjoint solve,
//!    `n_factorizations == 1`) matches a central finite difference of the
//!    full pipeline — with the Huygens-box surface tets **freshly rebuilt on
//!    the moved mesh** in the reference — to `rel_err ≤ 5e-3`,
//! 4. the Huygens box is stationary under the design motion (its surface-tet
//!    nodes are pinned and the surface selector is unchanged), the premise
//!    that makes `∂NTFF/∂X|_x = 0`,
//! 5. a conjugated cotangent is rejected by the same FD (the gate bites).

use std::f64::consts::PI;

use faer::c64;

use geode_core::constants::ETA_0_OHM;
use geode_core::driven::extraction::s11_sq_and_dg_dv;
use geode_core::driven::ports::{assemble_port_flux, port_current, port_voltage};
use geode_core::driven::scattering::{box_surface_tet_nodes, flux_power_box};
use geode_core::driven::shape::{
    chain_node_motion_pml_pinned, driven_shape_gradient_matched_upml_ports, pml_shell_nodes,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{
    CURVED_SMOKE_PML_THICK, FR4_MATERIALS, PHYS_UPML, read_patch_smoke_curved_fixture,
};
use geode_core::mesh::{TetMesh, pec_interior_mask_from_triangles};
use geode_core::postproc::ntff::{broadside_directivity, far_field_covectors, ntff_far_field};
use geode_core::testing::{TestBackend, test_device as device};

type B = TestBackend;

const SIGMA_0: f64 = 1.0;
const OMEGA: f64 = 0.35;
const FLUX_SHRINK: f64 = 0.10;
/// Observation direction of the FD gate (off the pole, off the φ seam).
const THETA0: f64 = 0.5;
const PHI0: f64 = 0.7;

#[test]
fn ntff_realized_gain_shape_gradient_matches_finite_difference() {
    let fixture = read_patch_smoke_curved_fixture().expect("curved smoke fixture");
    let mesh = fixture.mesh.clone();
    let edges = mesh.edges();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[
            fixture.patch_triangles().as_slice(),
            fixture.ground_triangles().as_slice(),
            fixture.outer_boundary_triangles().as_slice(),
        ],
    );
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };

    let pml_thick = CURVED_SMOKE_PML_THICK;
    let (air_lo, air_hi) = fixture.air_box(pml_thick);
    let (eps_t, nu_t) =
        fixture.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, OMEGA);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let box_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let box_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);

    let patch_port = fixture.port();
    let r_nat = 50.0 / ETA_0_OHM;
    let v_inc = c64::new(1.0, 0.0);
    let port = patch_port.lumped_port(r_nat, v_inc);
    let flux = assemble_port_flux(&mesh, &patch_port.faces, patch_port.e_hat, &edges);
    let inv_width = 1.0 / patch_port.width;
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };

    // ---- 1. Forward on the curved mesh: NTFF is physical. -----------------
    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &bcs,
        std::slice::from_ref(&port),
        OMEGA,
        &source,
        &device(),
    )
    .expect("curved forward solve");
    let v = port_voltage(&mesh, &port, &edges, &sol.e_edges);
    let i = port_current(&port, v);
    let p_in = 0.5 * (v * i.conj()).re;
    let p_rad = flux_power_box(&mesh, OMEGA, &sol.e_edges, box_lo, box_hi);
    let (s11_sq, _) = s11_sq_and_dg_dv(v, v_inc, r_nat, r_nat);
    assert!(s11_sq <= 1.0 + 1e-6, "passivity: |S11|² = {s11_sq}");
    assert!(p_in > 0.0, "accepted power must be positive, got {p_in}");

    let (n_theta, n_phi) = (61, 48);
    let ff = ntff_far_field(&mesh, OMEGA, &sol.e_edges, box_lo, box_hi, n_theta, n_phi);
    let d_bs = broadside_directivity(&ff);
    assert!(d_bs.is_finite() && d_bs > 0.0, "broadside D = {d_bs}");

    // ---- 2. Covectors == grid; intensity normalisation. --------------------
    let cov_bs = far_field_covectors(&mesh, OMEGA, box_lo, box_hi, 0.0, 0.0);
    let (a, b) = cov_bs.components(&sol.e_edges);
    assert!(
        (a - ff.e_theta[0]).norm() < 1e-9 * (1.0 + a.norm()),
        "covector E_theta vs grid"
    );
    assert!((b - ff.e_phi[0]).norm() < 1e-9 * (1.0 + b.norm()), "E_phi");

    // ∮U dΩ via the grid (same quadrature as `directivity`).
    let k2 = OMEGA * OMEGA / (32.0 * PI * PI);
    let mut integral = 0.0;
    for it in 0..n_theta {
        let w_t = if it == 0 {
            0.5 * (ff.theta[1] - ff.theta[0])
        } else if it == n_theta - 1 {
            0.5 * (ff.theta[n_theta - 1] - ff.theta[n_theta - 2])
        } else {
            0.5 * (ff.theta[it + 1] - ff.theta[it - 1])
        };
        for ip in 0..n_phi {
            integral += k2
                * ff.power_density(it, ip)
                * ff.theta[it].sin()
                * w_t
                * (2.0 * PI / n_phi as f64);
        }
    }
    eprintln!(
        "curved NTFF: D_bs = {d_bs:.4}, ∮U dΩ = {integral:.6e}, P_rad(flux) = {p_rad:.6e}, \
         P_acc = {p_in:.6e}, |S11|² = {s11_sq:.4}"
    );
    // Same Huygens surface, so the far-field integral must reproduce the
    // Poynting flux up to the surface-quadrature (centroid-rule) error of the
    // coarse mesh.
    assert!(
        (integral - p_rad).abs() <= 0.35 * p_rad.abs(),
        "∮U dΩ = {integral:.4e} vs P_rad = {p_rad:.4e}: far-field intensity normalisation off"
    );
    assert!(
        p_rad <= p_in * (1.0 + 1e-6),
        "radiated power exceeds accepted power"
    );

    // ---- 3. Realized-gain shape gradient vs central FD. -------------------
    let make_objective = |m: &TetMesh| {
        far_field_covectors(m, OMEGA, box_lo, box_hi, THETA0, PHI0).realized_gain_objective(
            flux.clone(),
            inv_width,
            v_inc,
            r_nat,
        )
    };
    let sg = driven_shape_gradient_matched_upml_ports::<B, _>(
        &mesh,
        &eps_t,
        &nu_t,
        &bcs,
        std::slice::from_ref(&port),
        OMEGA,
        &source,
        make_objective(&mesh),
        &device(),
    )
    .expect("NTFF-FOM shape gradient");
    assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
    assert!(sg.residual_rel < 1e-8, "residual {:.3e}", sg.residual_rel);
    assert!(sg.objective > 0.0, "realized gain must be positive");

    // Pinned set: PML shell + port faces + Huygens-box surface tets.
    let pml_tet_mask: Vec<bool> = fixture
        .tet_physical_tags
        .iter()
        .map(|&t| t == PHYS_UPML)
        .collect();
    let mut pinned = pml_shell_nodes(&mesh, &pml_tet_mask);
    for tri in &patch_port.faces {
        for &n in tri {
            pinned[n as usize] = true;
        }
    }
    let box_nodes = box_surface_tet_nodes(&mesh, box_lo, box_hi);
    for (p, &bn) in pinned.iter_mut().zip(&box_nodes) {
        *p |= bn;
    }

    // Motion: +z Gaussian bump on the free nodes (centred on their centroid).
    let mut c = [0.0_f64; 3];
    let mut n_free = 0.0;
    for (i, p) in mesh.nodes.iter().enumerate() {
        if !pinned[i] {
            for k in 0..3 {
                c[k] += p[k];
            }
            n_free += 1.0;
        }
    }
    assert!(n_free > 10.0, "need free nodes to move");
    for ck in c.iter_mut() {
        *ck /= n_free;
    }
    let s2 = 25.0_f64;
    let d: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if pinned[i] {
                [0.0; 3]
            } else {
                let r2: f64 = (0..3).map(|k| (p[k] - c[k]).powi(2)).sum();
                [0.0, 0.0, (-r2 / (2.0 * s2)).exp()]
            }
        })
        .collect();

    // Huygens-box stationarity: box nodes do not move, and the surface
    // selector is unchanged on the moved mesh.
    let moved_at = |theta: f64| -> TetMesh {
        let mut m = mesh.clone();
        for (node, dn) in m.nodes.iter_mut().zip(&d) {
            for k in 0..3 {
                node[k] += theta * dn[k];
            }
        }
        m
    };
    for (n, &bn) in box_nodes.iter().enumerate() {
        if bn {
            assert_eq!(d[n], [0.0; 3], "design motion moves a Huygens-box node");
        }
    }
    let moved_big = moved_at(1.0);
    assert_eq!(
        box_surface_tet_nodes(&moved_big, box_lo, box_hi),
        box_nodes,
        "Huygens-box surface selector changed under the design motion"
    );

    let g_of = |theta: f64| -> f64 {
        let m = moved_at(theta);
        let sol = driven_solve_with_ports::<B>(
            &m,
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &eps_t,
                nu_tensor: &nu_t,
            },
            None,
            &bcs,
            std::slice::from_ref(&port),
            OMEGA,
            &source,
            &device(),
        )
        .expect("moved forward");
        // Covectors REBUILT on the moved mesh: an independent check that the
        // near-to-far map really is geometry-constant.
        make_objective(&m)(&sol.e_edges).0
    };

    let g0 = g_of(0.0);
    assert!(
        (g0 - sg.objective).abs() <= 1e-8 * g0.abs().max(1.0),
        "objective mismatch: adjoint {} vs forward {g0}",
        sg.objective
    );
    let h = 1e-6;
    let ana = chain_node_motion_pml_pinned(&sg.grad_node, &d, &pinned);
    let fd = (g_of(h) - g_of(-h)) / (2.0 * h);
    assert!(fd.abs() > 1e-8, "FD gradient {fd} ~ 0 (degenerate)");
    let rel = (ana - fd).abs() / fd.abs();
    eprintln!("NTFF realized-gain FD gate: ana {ana:.6e}, fd {fd:.6e}, rel_err {rel:.3e}");
    assert!(
        rel <= 5e-3,
        "adjoint {ana} vs FD {fd}: rel_err {rel:.3e} > 5e-3"
    );

    // ---- 5. Conjugation tripwire. -----------------------------------------
    let good = make_objective(&mesh);
    let wrong = move |x: &[c64]| -> (f64, Vec<c64>) {
        let (g, cot) = good(x);
        (g, cot.iter().map(|z| z.conj()).collect())
    };
    let sg_wrong = driven_shape_gradient_matched_upml_ports::<B, _>(
        &mesh,
        &eps_t,
        &nu_t,
        &bcs,
        std::slice::from_ref(&port),
        OMEGA,
        &source,
        wrong,
        &device(),
    )
    .expect("wrong-cotangent gradient");
    let ana_wrong = chain_node_motion_pml_pinned(&sg_wrong.grad_node, &d, &pinned);
    let rel_wrong = (ana_wrong - fd).abs() / fd.abs();
    assert!(
        rel_wrong > 1e-2,
        "conjugation error not detected (rel {rel_wrong:.3e}); gate does not bite"
    );
}

/// Pins the committed demo artifact
/// `benchmarks/patch_antenna_conformal/ntff_gain_results.toml` (regenerate with
/// `cargo run -p geode-core --release --example patch_conformal_ntff_diffopt`)
/// against silent drift: FD gate passed with one factorization, passivity and
/// the non-inversion guard held, the Huygens box was stationary, and the
/// recorded outcome is internally consistent (honest negatives are allowed).
#[test]
fn committed_ntff_gain_artifact_is_consistent() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/patch_antenna_conformal/ntff_gain_results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let v: toml::Value = toml::from_str(&text).expect("parse ntff_gain_results.toml");
    let fd = &v["fd_gate"];
    assert!(fd["rel_err"].as_float().unwrap() <= fd["tol"].as_float().unwrap());
    assert_eq!(fd["n_factorizations"].as_integer(), Some(1));
    let r = &v["result"];
    assert_eq!(r["passive"].as_bool(), Some(true));
    assert_eq!(r["huygens_box_stationary"].as_bool(), Some(true));
    assert!(
        r["min_vol_ratio"].as_float().unwrap() >= r["min_vol_ratio_budget"].as_float().unwrap()
    );
    let improved = r["improved"].as_bool().unwrap();
    let (g0, g1) = (
        r["gain_initial"].as_float().unwrap(),
        r["gain_final"].as_float().unwrap(),
    );
    assert_eq!(
        improved,
        g1 > g0 * (1.0 + 1e-6),
        "improved flag inconsistent"
    );
    assert_eq!(
        r["outcome"].as_str().unwrap(),
        if improved {
            "gain_improved"
        } else {
            "honest_negative"
        }
    );
    let hist = v["history"].as_array().expect("history");
    assert_eq!(hist.len() as i64, r["n_steps"].as_integer().unwrap() + 1);
    let mut prev = f64::NEG_INFINITY;
    for h in hist {
        let g = h["gain"].as_float().unwrap();
        assert!(g >= prev - 1e-12, "ascent history must be monotone");
        assert!(
            h["s11_db"].as_float().unwrap() <= 1e-6,
            "passivity |S11| <= 0 dB"
        );
        prev = g;
    }
}
