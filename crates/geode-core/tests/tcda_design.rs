//! Issue #1578 (Epic #1252 Phase 4c, child 1 of #1537): the lumped-feed
//! `Γ_active` adjoint and the Design A min-max driver.
//!
//! Four checks, in the order they establish the chain:
//!
//! 1. **Finite differences on the adjoint.** `floquet_edge_gamma_vjp` vs a
//!    central difference of `|Γ|²` on the scaled baseline cell at one
//!    oblique grid point, over a 5-parameter design: the four outline
//!    columns of a one-coefficient-per-edge `FloquetElementParam` plus the
//!    superstrate `ε′`. The right and left columns both move the feed-edge
//!    nodes, which is the case the earlier spec wrongly wanted fenced off.
//! 2. **The baseline reproduces.** At zero outline displacement with the
//!    superstrate fixed at the baseline `ε′ = 2.0`, the driver's `|Γ|` at
//!    the three `RECORDED_SUB` points of `tcda_baseline.rs` matches the
//!    recorded dB within 0.05 dB — i.e. the millimetre scale did not change
//!    the physics.
//! 3. **An inverted mesh is an error.** A large enough outline displacement
//!    folds tets; `values()` must return an `EvalError`, not a number.
//! 4. **MMA moves the worst case down.** A few iterations on a reduced grid
//!    lower the worst-case `|Γ|` and keep the trace and gap constraints
//!    feasible.
//! 5. **The recorded Design A run reproduces** (#1579). The committed
//!    `benchmarks/tcda_design/results.toml` — written by
//!    `examples/tcda_design_run.rs`, the hours-long full run — is re-solved
//!    at three of its fifty points, including the recorded worst one,
//!    within the same 0.05 dB. The record is read, never restated here, so
//!    the pin cannot drift from the number the README quotes.
//!
//! The direct solves are release-only, like `tcda_baseline.rs`.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;
#[path = "common/tcda_baseline.rs"]
mod tcda_baseline;
#[path = "common/tcda_design.rs"]
mod tcda_design;

use std::f64::consts::PI;

use faer::c64;

use geode_core::driven::floquet_sensitivity::{
    FloquetDesign, FloquetGeometry, FloquetSensitivityError, FloquetSensitivityOptions,
    floquet_edge_gamma_vjp,
};
use geode_core::driven::s_sensitivity::{MaterialDesign, SParam, ShapeDesign};
use geode_core::shape::floquet_element::{ElementGeometry, FloquetElementParam, StackLayer};
use geode_optimize::{Bounds, MinMaxPoints, Mma, MmaOptions, Objective, Optimizer, StopCriteria};

use tcda_baseline::{BaselineParams, ScanGrid, build_cell_scaled};
use tcda_design::{ConstrainedEpigraph, MM_PER_UNIT, ParamSpec, ScanPoint, TcdaDesign};

/// The recorded baseline element (identical to `tcda_baseline.rs`'s `P`).
const P: BaselineParams = BaselineParams {
    pitch: 0.9,
    half_len: 0.39,
    strip_cells: 2,
    n: 14,
    dz: 0.125,
    l_dip: 4,
    l_sub: 2,
    l_air: 2,
    eps_sub: 2.0,
};

/// Feed-line reference impedance of the recorded baseline.
const Z_REF: f64 = 0.15;

/// Recorded `|Γ|` (dB) at the sub-grid points, `(ω/π, θ°, φ°, dB)`: the
/// same three points `tcda_baseline.rs` asserts.
const RECORDED_SUB: [(f64, f64, f64, f64); 3] = [
    (1.0, 0.0, 0.0, -10.48),
    (1.1, 60.0, 0.0, -5.68),
    (1.1, 60.0, 90.0, -3.50),
];
/// Tolerance on the recorded dB values.
const TOL_DB: f64 = 0.05;

/// A one-coefficient-per-edge parameterization of the scaled element: four
/// outline columns (right, left, top, bottom), no material block.
fn outline_param(geo: ElementGeometry) -> FloquetElementParam {
    FloquetElementParam::new(geo, vec![], 1, 4, 20.0).expect("the scaled element is well formed")
}

/// `ElementGeometry` of the scaled baseline element.
fn element_geometry(parts: &tcda_baseline::BaselineParts) -> ElementGeometry {
    let g = parts.geom;
    ElementGeometry {
        period: [g.pitch, g.pitch],
        z_range: [0.0, g.h_top],
        z_copper: g.h_dip,
        patch_x: g.patch_x,
        patch_y: g.patch_y,
    }
}

// ------------------------------------------------------------------ 1. FD

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn edge_gamma_vjp_matches_central_differences_at_an_oblique_point() {
    // The scaled element, in millimetres.
    let c = build_cell_scaled(&P, MM_PER_UNIT);
    let parts = &c.parts;
    let param = outline_param(element_geometry(parts));
    assert_eq!(param.n_params(), 4, "four outline columns");
    let columns = param.outline_columns(&parts.mesh);

    // Five parameters: the superstrate ε′ first (the adjoint's own column
    // order), then right, left, top, bottom. Built directly, because the
    // baseline's ε′ = 2.0 superstrate slab is 3.75 mm thick and so is not a
    // STANDARD_STACK layer at any stock thickness: the gradient is still
    // well defined, only the stack *constraint* needs a real laminate.
    let layer_of_tet: Vec<Option<usize>> = parts
        .superstrate
        .iter()
        .map(|&inside| inside.then_some(0))
        .collect();
    let design = FloquetDesign {
        material: Some(
            MaterialDesign::from_regions(layer_of_tet, vec!["superstrate".into()])
                .unwrap()
                .with_components(true, false),
        ),
        shape: Some(
            ShapeDesign::from_columns(
                columns.clone(),
                vec![
                    "right0".into(),
                    "left0".into(),
                    "top0".into(),
                    "bottom0".into(),
                ],
            )
            .unwrap(),
        ),
    };
    let n_params = 5;

    // One oblique grid point of the Phase-4 grid (the recorded H-plane
    // worst case), at the mesh's own ω.
    let (omega_n, theta, phi) = (1.1 * PI, 60f64.to_radians(), PI / 2.0);
    let omega = parts.omega_of(omega_n);

    // The feed-edge nodes DO move under the right and left columns: that is
    // the verified correction this issue carries, so assert it rather than
    // assume it (the weight is small, ~0.017 on this mesh).
    let feed_nodes = {
        let edges = parts.mesh.edges();
        edges[parts.feed_edge]
    };
    for (col, name) in [(0usize, "right0"), (1, "left0")] {
        let moved = feed_nodes
            .iter()
            .map(|&nd| columns[col][nd as usize][0].abs())
            .fold(0.0_f64, f64::max);
        assert!(
            moved > 1e-6,
            "{name} must move a feed-edge node (got {moved:.3e}); this issue's \
             parameterization depends on that being differentiated, not fenced"
        );
    }

    // θ₀: the baseline element (zero displacement, ε′ = 2.0).
    let theta0: Vec<f64> = {
        let mut t = vec![0.0; n_params];
        t[0] = P.eps_sub;
        t
    };

    // `θ ↦ (nodes, eps)`: outline columns 0..4 live at θ[1..5].
    let morph = |th: &[f64]| -> (Vec<[f64; 3]>, Vec<c64>) {
        let mut nodes = parts.mesh.nodes.clone();
        for (ci, col) in columns.iter().enumerate() {
            let t = th[1 + ci];
            if t == 0.0 {
                continue;
            }
            for (nd, v) in col.iter().enumerate() {
                for k in 0..3 {
                    nodes[nd][k] += t * v[k];
                }
            }
        }
        let eps: Vec<c64> = parts
            .superstrate
            .iter()
            .map(|&inside| c64::new(if inside { th[0] } else { 1.0 }, 0.0))
            .collect();
        (nodes, eps)
    };

    let gamma_sq = |th: &[f64]| -> f64 {
        let (nodes, eps) = morph(th);
        let cell = parts.assemble(&nodes, &eps).unwrap();
        let z = cell
            .edge_scan_impedance(omega, theta, phi, parts.feed_edge)
            .unwrap();
        let zr = c64::new(Z_REF, 0.0);
        ((z - zr) / (z + zr)).norm_sqr()
    };

    // The adjoint at θ₀.
    let (nodes0, eps0) = morph(&theta0);
    let cell0 = parts.assemble(&nodes0, &eps0).unwrap();
    let mut mesh0 = parts.mesh.clone();
    mesh0.nodes.copy_from_slice(&nodes0);
    let n0 = cell0.factorization_count();
    let geo0 = FloquetGeometry {
        mesh: &mesh0,
        map: &parts.map,
        eps_r: &eps0,
        sigma: None,
    };
    let vjp = floquet_edge_gamma_vjp(
        &cell0,
        &geo0,
        omega,
        theta,
        phi,
        parts.feed_edge,
        Z_REF,
        &design,
        &FloquetSensitivityOptions::default(),
    )
    .unwrap();
    assert_eq!(vjp.grad.len(), n_params);
    assert_eq!(
        vjp.params,
        vec![
            SParam::EpsPrime { region: 0 },
            SParam::Shape { column: 0 },
            SParam::Shape { column: 1 },
            SParam::Shape { column: 2 },
            SParam::Shape { column: 3 },
        ],
        "ε′ first, then the outline columns: the θ order of #1535"
    );
    // One factorization per (ω, θ, φ): the adjoint rides the forward LU.
    assert_eq!(vjp.n_factorizations, 1, "the adjoint shares the forward LU");
    assert_eq!(cell0.factorization_count() - n0, 1);
    assert_eq!(vjp.n_adjoint_solves, 1);

    // Γ of the adjoint is the forward Γ.
    let z_fwd = cell0
        .edge_scan_impedance(omega, theta, phi, parts.feed_edge)
        .unwrap();
    assert!(
        (vjp.z - z_fwd).norm() <= 1e-10 * z_fwd.norm().max(1e-30),
        "Z {} vs forward {z_fwd}",
        vjp.z
    );
    assert!(
        (vjp.objective - gamma_sq(&theta0)).abs() < 1e-12,
        "|Γ|² {} vs forward {}",
        vjp.objective,
        gamma_sq(&theta0)
    );
    eprintln!(
        "θ₀: Z = {:.6} |Γ|² = {:.6} ({:.2} dB)",
        vjp.z,
        vjp.objective,
        10.0 * vjp.objective.log10()
    );

    // Central differences. The ε′ step is dimensionless, the outline steps
    // are millimetres on a 13.5 mm cell.
    let steps = [1e-4, 2e-3, 2e-3, 2e-3, 2e-3];
    for i in 0..n_params {
        let h = steps[i];
        let mut up = theta0.clone();
        up[i] += h;
        let mut dn = theta0.clone();
        dn[i] -= h;
        let fd = (gamma_sq(&up) - gamma_sq(&dn)) / (2.0 * h);
        let ad = vjp.grad[i];
        let rel = (ad - fd).abs() / fd.abs().max(1e-30);
        eprintln!(
            "{:?}: AD {ad:.6e}  FD {fd:.6e}  rel {rel:.2e}",
            vjp.params[i]
        );
        assert!(
            fd.abs() > 1e-6,
            "{:?}: a vanishing derivative proves nothing (FD {fd:.3e})",
            vjp.params[i]
        );
        assert!(rel <= 5e-3, "{:?}: rel_err {rel:.3e} > 5e-3", vjp.params[i]);
    }
}

/// The remaining fence: a column that moves a **port-face** node is still
/// the typed error, not a silently wrong gradient. (A column that moves the
/// *feed* edge is legitimate and is exercised by the FD check above.)
#[test]
fn a_column_that_moves_a_port_node_is_still_a_typed_error() {
    let c = build_cell_scaled(&P, MM_PER_UNIT);
    let parts = &c.parts;
    // One column that lifts every node of the aperture port face.
    let mut col = vec![[0.0_f64; 3]; parts.mesh.n_nodes()];
    let tol = 1e-9 * MM_PER_UNIT;
    for (i, p) in parts.mesh.nodes.iter().enumerate() {
        if (p[2] - parts.geom.h_top).abs() < tol {
            col[i] = [0.0, 0.0, 1.0];
        }
    }
    assert!(
        col.iter().any(|v| v[2] != 0.0),
        "the aperture plane has nodes"
    );
    let design = FloquetDesign {
        material: None,
        shape: Some(ShapeDesign::from_columns(vec![col], vec!["lift".into()]).unwrap()),
    };
    let eps = parts.eps.clone();
    let err = floquet_edge_gamma_vjp(
        &c.cell,
        &FloquetGeometry {
            mesh: &parts.mesh,
            map: &parts.map,
            eps_r: &eps,
            sigma: None,
        },
        parts.omega_of(PI),
        0.0,
        0.0,
        parts.feed_edge,
        Z_REF,
        &design,
        &FloquetSensitivityOptions::default(),
    )
    .expect_err("moving a Floquet port face must be refused");
    match err {
        FloquetSensitivityError::ShapeMovesPinnedNode { what, .. } => {
            assert_eq!(what, "Floquet port face");
        }
        other => panic!("expected ShapeMovesPinnedNode, got {other}"),
    }
}

/// A non-positive `z_ref` is rejected rather than producing a pole.
#[test]
fn a_non_positive_reference_impedance_is_refused() {
    let c = build_cell_scaled(&P, MM_PER_UNIT);
    let parts = &c.parts;
    let param = outline_param(element_geometry(parts));
    let design = FloquetDesign {
        material: None,
        shape: Some(
            ShapeDesign::from_columns(
                param.outline_columns(&parts.mesh),
                (0..4).map(|i| format!("c{i}")).collect(),
            )
            .unwrap(),
        ),
    };
    let eps = parts.eps.clone();
    for bad in [0.0, -0.15, f64::NAN] {
        assert!(
            floquet_edge_gamma_vjp(
                &c.cell,
                &FloquetGeometry {
                    mesh: &parts.mesh,
                    map: &parts.map,
                    eps_r: &eps,
                    sigma: None,
                },
                parts.omega_of(PI),
                0.0,
                0.0,
                parts.feed_edge,
                bad,
                &design,
                &FloquetSensitivityOptions::default(),
            )
            .is_err(),
            "z_ref = {bad} must be refused"
        );
    }
}

// ------------------------------------------------- 2. baseline reproduction

/// The driver at **zero outline displacement with the superstrate at the
/// baseline `ε′ = 2.0`** reproduces the recorded numbers. Not at
/// `initial_theta()` with a layer declared: `initial_theta` puts a layer's
/// `ε′` at its laminate midpoint and the lowest `STANDARD_STACK` range
/// starts at 2.97 (RO3003), so no standard layer can sit at 2.0. The run
/// therefore declares **no** material layers and keeps the baseline stack.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn baseline_reproduces_at_zero_displacement_and_eps_2() {
    // One single-point grid per recorded point: the driver's grid is the
    // full Cartesian product, so three separate drivers cost three solves
    // where one three-by-three-by-three grid would cost twenty-seven.
    for (w, th, ph, want) in RECORDED_SUB {
        let grid = ScanGrid {
            omegas: vec![w * PI],
            thetas: vec![th.to_radians()],
            phis: vec![ph.to_radians()],
        };
        let mut d = TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), Z_REF, &grid).unwrap();

        // The scale is what makes the 0.15 mm limits meaningful: assert the
        // element really is a millimetre-scale PCB cell.
        let g = d.parts().geom;
        assert!(
            (g.pitch - 0.9 * MM_PER_UNIT).abs() < 1e-9,
            "pitch {} mm",
            g.pitch
        );
        assert!(
            (13.0..14.0).contains(&g.pitch),
            "a 10 GHz TCDA cell should be ~13.5 mm, got {}",
            g.pitch
        );
        assert!(
            (d.parts().half_len - 0.39 * MM_PER_UNIT).abs() < 1e-9,
            "realized half-length {} mm",
            d.parts().half_len
        );

        // No material layers: θ₀ is all zeros and `dim` is the outline only.
        assert_eq!(d.dim(), 4);
        let theta0 = d.initial_theta();
        assert!(theta0.iter().all(|&v| v == 0.0), "{theta0:?}");

        // The baseline stack is untouched at θ₀.
        let (nodes, eps) = d.morph(&theta0).unwrap();
        assert_eq!(nodes, d.parts().mesh.nodes, "zero displacement");
        assert_eq!(eps, d.parts().eps, "the baseline superstrate");

        // The scaled baseline must also be manufacturable, or the whole
        // point of fixing a scale is lost.
        let cons = d.param().constraints(&theta0).unwrap();
        assert!(
            !cons.is_violated(),
            "the scaled baseline must be manufacturable: {:?} (max {:.4} mm)",
            cons.g,
            cons.max_violation()
        );

        let vals = MinMaxPoints::values(&mut d, &theta0).unwrap();
        assert_eq!(vals.len(), 1);
        let db = 10.0 * vals[0].log10();
        eprintln!("ω/π={w} θ={th}° φ={ph}°: {db:.2} dB (recorded {want})");
        assert!(
            vals[0].sqrt() <= 1.0 + 1e-6,
            "|Γ| > 1 at ω/π={w} θ={th}° φ={ph}°"
        );
        assert!(
            (db - want).abs() < TOL_DB,
            "ω/π={w} θ={th}° φ={ph}°: {db:.3} dB vs recorded {want} dB — the \
             millimetre scale changed the physics"
        );
    }
}

// ------------------------------------------------------- 3. inverted mesh

#[test]
fn an_inverted_morph_is_an_evaluation_error_not_a_number() {
    let grid = ScanGrid {
        omegas: vec![PI],
        thetas: vec![0.0],
        phis: vec![0.0],
    };
    let mut d = TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), Z_REF, &grid).unwrap();
    let mut theta = d.initial_theta();
    // Push the right edge far past the cell wall: the taper's outboard
    // nodes cross each other and tets fold.
    theta[0] = 50.0 * d.parts().geom.pitch;
    let err = MinMaxPoints::values(&mut d, &theta)
        .expect_err("an inverted mesh must not produce a number");
    assert!(
        err.message.contains("inverted") || err.message.contains("degenerate"),
        "unhelpful message: {}",
        err.message
    );
    // `morph` is the gate, so the gradient path refuses the same design.
    assert!(d.morph(&theta).is_err());
    assert!(MinMaxPoints::gradient(&mut d, 0, &theta).is_err());
    // And a finite, modest displacement is still accepted.
    let mut ok = d.initial_theta();
    ok[0] = 0.05;
    assert!(d.morph(&ok).is_ok(), "a 0.05 mm step must stay valid");
}

#[test]
fn the_driver_rejects_a_multi_layer_stack_and_a_bad_z_ref() {
    let grid = ScanGrid {
        omegas: vec![PI],
        thetas: vec![0.0],
        phis: vec![0.0],
    };
    let two = vec![
        StackLayer {
            material: 3,
            thickness_mm: 0.5,
        },
        StackLayer {
            material: 3,
            thickness_mm: 0.5,
        },
    ];
    assert!(
        TcdaDesign::new(
            &P,
            MM_PER_UNIT,
            ParamSpec {
                layers: two,
                ..ParamSpec::default()
            },
            Z_REF,
            &grid
        )
        .is_err()
    );
    assert!(TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), 0.0, &grid).is_err());
    let empty = ScanGrid {
        omegas: vec![],
        thetas: vec![],
        phis: vec![],
    };
    assert!(TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), Z_REF, &empty).is_err());
}

// ------------------------------------------------------------ 4. MMA smoke

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn a_few_mma_iterations_lower_the_worst_case_and_stay_feasible() {
    // A reduced grid: the band-top worst scan angle in both planes (the
    // recorded worst case is the H-plane one). Two points, so two
    // factorizations per forward pass and one per gradient.
    let grid = ScanGrid {
        omegas: vec![1.1 * PI],
        thetas: vec![60f64.to_radians()],
        phis: vec![0.0, PI / 2.0],
    };
    let d = TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), Z_REF, &grid).unwrap();
    let n = d.dim();
    assert_eq!(n, 4);
    let mut obj = ConstrainedEpigraph::new(d).unwrap();
    // 4 KS-folded geometric rows (trace x/y, gap x/y), no layer rows.
    assert_eq!(obj.n_element_constraints(), 4);
    assert_eq!(obj.n_constraints(), 2 + 4);

    let theta0 = vec![0.0; n];
    let d0 = obj.initial_design(&theta0).unwrap();
    let worst0 = d0[n];
    eprintln!(
        "θ₀: worst |Γ|² = {worst0:.6} ({:.2} dB)",
        10.0 * worst0.log10()
    );

    // Outline displacements in millimetres; +-1 mm is well inside the
    // 13.5 mm cell and inside the tapers' validity.
    let mut lo = vec![-1.0; n];
    let mut hi = vec![1.0; n];
    lo.push(0.0);
    hi.push(4.0 * worst0.max(1e-3));
    let bounds = Bounds::new(lo, hi).unwrap();

    let mma = Mma::from_options(MmaOptions {
        stop: StopCriteria {
            max_iter: 6,
            ..Default::default()
        },
        ..Default::default()
    });
    let st = mma.minimize(&mut obj, &d0, &bounds).unwrap();
    let theta = st.x()[..n].to_vec();
    let t = st.x()[n];
    eprintln!(
        "after {} iters: t = {t:.6}, θ = {theta:?} ({:?})",
        st.progress.history.len(),
        st.progress.status
    );

    // The worst case went down (re-evaluated, not read off t).
    let ev = obj.evaluate(st.x()).unwrap();
    let worst = ev.constraints[..2]
        .iter()
        .map(|g| g + t)
        .fold(f64::NEG_INFINITY, f64::max);
    eprintln!(
        "worst |Γ|²: {worst0:.6} -> {worst:.6}  ({:.2} dB -> {:.2} dB)",
        10.0 * worst0.log10(),
        10.0 * worst.log10()
    );
    assert!(
        worst < worst0 - 1e-6,
        "MMA did not lower the worst case: {worst0:.6} -> {worst:.6}"
    );

    // ...and the manufacturability constraints are still feasible. The KS
    // fold never under-reports, so `≤ 0` on the folded rows is sound.
    let geom = &ev.constraints[2..];
    eprintln!("trace/gap rows at the optimum: {geom:?}");
    assert!(
        geom.iter().all(|&g| g <= 0.0),
        "trace/gap violated: {geom:?}"
    );
    let cons = obj.points().param().constraints_sampled(&theta).unwrap();
    assert!(
        !cons.is_violated(),
        "a sampled trace/gap constraint is violated: max {:.4} mm",
        cons.max_violation()
    );
}

// ------------------------------------------------- 5. #1579 run pin

/// The committed Design A run record (`benchmarks/tcda_design/results.toml`,
/// written by `examples/tcda_design_run.rs`).
fn design_record() -> toml::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/tcda_design/results.toml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading the #1579 run record at {}: {e}", path.display()));
    toml::from_str(&raw).expect("the #1579 run record is valid TOML")
}

fn num(v: &toml::Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
        .unwrap_or_else(|| panic!("{key} is not a number in the #1579 run record"))
}

fn rec_f64(doc: &toml::Value, table: &str, key: &str) -> f64 {
    num(&doc[table], key)
}

/// The recorded per-point dB at `(ω/π, θ°, φ°)`, read out of the record's
/// own tables (rows `ω/π = 0.9..1.1`, columns `θ = 0,15,30,45,60`).
fn recorded_db(doc: &toml::Value, w: f64, theta_deg: f64, phi_deg: f64) -> f64 {
    let plane = if phi_deg == 0.0 {
        "table_e_plane_db"
    } else {
        "table_h_plane_db"
    };
    let row = ((w - 0.9) / 0.05).round() as usize;
    let col = (theta_deg / 15.0).round() as usize;
    doc["run"][plane][row][col]
        .as_float()
        .unwrap_or_else(|| panic!("[run].{plane}[{row}][{col}] is not a float"))
}

/// **The #1579 pin.** Rebuilds the driver with the recorded `n_edge`,
/// loads the committed final design vector, and re-solves three grid
/// points — including the recorded worst one — against the recorded
/// table, within the same `TOL_DB = 0.05` the baseline pin uses.
///
/// It also re-checks, without any solve, the two properties AC 2 of #1579
/// records: no tet inverts at the final design, and every #1535
/// manufacturability constraint is feasible there.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn the_recorded_design_run_reproduces_its_worst_case() {
    let doc = design_record();

    let n_edge = doc["design"]["n_edge"]
        .as_integer()
        .expect("[design].n_edge") as usize;
    let theta: Vec<f64> = doc["run"]["final_theta_mm"]
        .as_array()
        .expect("[run].final_theta_mm")
        .iter()
        .map(|v| v.as_float().expect("a float"))
        .collect();
    assert_eq!(
        theta.len(),
        4 * n_edge,
        "the record's design vector must be outline-only: 4*n_edge entries"
    );

    // A one-point grid is enough to build the driver; `gamma_at` takes an
    // arbitrary `ScanPoint`, so the pin re-solves exactly the points it
    // names rather than a whole Cartesian product.
    let grid = ScanGrid {
        omegas: vec![PI],
        thetas: vec![0.0],
        phis: vec![0.0],
    };
    let mut d = TcdaDesign::new(
        &P,
        MM_PER_UNIT,
        ParamSpec {
            layers: Vec::new(),
            n_edge,
            n_samples: 4,
            beta: 20.0,
        },
        Z_REF,
        &grid,
    )
    .expect("the recorded parameterization rebuilds");
    assert_eq!(d.dim(), theta.len(), "the record matches the driver's θ");

    // No solve: the mesh is valid and the design is manufacturable.
    let (nodes, _) = d
        .morph(&theta)
        .expect("the recorded design must morph without inverting a tet");
    let mut moved = d.parts().mesh.clone();
    moved.nodes.copy_from_slice(&nodes);
    let ratio = geode_core::shape::min_tet_volume_ratio(&d.parts().mesh, &moved);
    let recorded_ratio = rec_f64(&doc, "run", "min_tet_volume_ratio");
    assert!(
        (ratio - recorded_ratio).abs() < 1e-3,
        "min_tet_volume_ratio {ratio:.6} vs recorded {recorded_ratio:.6}"
    );
    let cons = d
        .param()
        .constraints_sampled(&theta)
        .expect("the sampled constraints");
    assert!(
        !cons.is_violated(),
        "the recorded design violates a #1535 trace/gap limit by {:.4} mm",
        cons.max_violation()
    );

    // Three points: the recorded worst case, broadside at band centre, and
    // the E-plane band-top scan — one from each regime the record tabulates.
    let wp = &doc["run"]["final_worst_point"];
    let worst = (
        num(wp, "omega_over_pi"),
        num(wp, "theta_deg"),
        num(wp, "phi_deg"),
    );
    let points = [worst, (1.0, 0.0, 0.0), (1.1, 60.0, 0.0)];

    let (cell, _, _) = d.assemble(&theta).expect("the recorded design assembles");
    let mut re_solved = Vec::new();
    for (w, th, ph) in points {
        let g = d
            .gamma_at(
                &cell,
                ScanPoint {
                    omega: w * PI,
                    theta: th.to_radians(),
                    phi: ph.to_radians(),
                },
            )
            .expect("the Floquet solve");
        let db = 20.0 * g.norm().log10();
        let want = recorded_db(&doc, w, th, ph);
        eprintln!("ω/π={w} θ={th}° φ={ph}°: {db:.2} dB (recorded {want:.2})");
        assert!(
            g.norm() <= 1.0 + 1e-6,
            "passivity: |Γ| = {} > 1 at ω/π={w} θ={th}° φ={ph}°",
            g.norm()
        );
        assert!(
            (db - want).abs() < TOL_DB,
            "ω/π={w} θ={th}° φ={ph}°: {db:.3} dB vs recorded {want:.3} dB"
        );
        re_solved.push(db);
    }

    // The recorded worst case is the worst of the three (it is the worst of
    // all fifty), so the headline number is pinned too.
    let worst_db = rec_f64(&doc, "run", "final_worst_db");
    let got = re_solved.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v));
    assert!(
        (got - worst_db).abs() < TOL_DB,
        "recorded worst case {worst_db:.3} dB, re-solved {got:.3} dB"
    );

    // And the verdict the record states follows from that number.
    let baseline = rec_f64(&doc, "verdict", "baseline_worst_db");
    let improvement = rec_f64(&doc, "verdict", "improvement_db");
    assert!(
        (baseline - worst_db - improvement).abs() < 1e-9,
        "[verdict].improvement_db must be baseline_worst_db - final_worst_db"
    );
    let stated = doc["verdict"]["kill_criterion"]
        .as_str()
        .expect("[verdict].kill_criterion");
    let expect = if improvement < 1.0 { "kill" } else { "pass" };
    assert_eq!(
        stated, expect,
        "the recorded verdict must follow from the recorded improvement of {improvement:.2} dB"
    );
}
