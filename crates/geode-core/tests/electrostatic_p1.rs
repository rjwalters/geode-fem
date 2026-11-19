//! Hand-computed fixtures for the scalar P1 electrostatic residual
//! estimator (issue #1765, parent #1481).
//!
//! Every expected value below is derived analytically from the estimator's
//! documented conventions (`crates/geode-core/src/adapt/electrostatic_p1.rs`):
//! the cell term `h_T²ρ_T²V_T`, the interior-face jump
//! `J_F = ε_{T⁺}∇φ|_{T⁺}·n̂ − ε_{T⁻}∇φ|_{T⁻}·n̂` (n̂ from the lower-index
//! tet to the higher-index one) split ½/½ between its tets, the natural
//! boundary flux `J_F = −ε_T∇φ|_T·n_out`, and prescribed faces (all three
//! nodes Dirichlet) skipped.
//!
//! Fixtures:
//!
//! 1. **Unit cube**, 6-tet Freudenthal triangulation — a uniform-gradient
//!    field `φ = z` with compatible boundary data (Dirichlet plates at
//!    `z = 0, 1`, natural side walls the field runs parallel to) has zero
//!    residual to roundoff.
//! 2. **Triangular bipyramid** — two tets sharing the face `ABC` in the
//!    plane `z = 0` (A=(0,0,0), B=(1,0,0), C=(0,1,0), D=(0,0,−1),
//!    E=(0,0,1)); tet 0 = A,B,D,C is T⁻, tet 1 = A,B,C,E is T⁺. With
//!    `φ_D = −1`, `φ_E = +1` and `φ_{ABC} = 0` the gradients are exactly
//!    `∇φ|_{T⁻} = +ẑ`, `∇φ|_{T⁺} = φ_E ẑ`, so any jump/flux is a clean
//!    multiple of `ε₀`.

use geode_core::adapt::electrostatic_p1::{ElectrostaticP1Error, estimate_electrostatic_p1};
use geode_core::assembly::electrostatic::{EPS_0, Electrode};
use geode_core::mesh::TetMesh;

/// Relative-tolerance closeness used throughout (house 1e-12 style).
fn close(a: f64, b: f64, scale: f64) {
    assert!((a - b).abs() <= 1e-12 * scale, "{a} vs {b} (scale {scale})");
}

/// The two-tet bipyramid: tet 0 = A,B,D,C (`T⁻`, below the shared face),
/// tet 1 = A,B,C,E (`T⁺`, above). Both positively oriented.
fn bipyramid() -> TetMesh {
    TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0],  // 0: A
            [1.0, 0.0, 0.0],  // 1: B
            [0.0, 1.0, 0.0],  // 2: C
            [0.0, 0.0, -1.0], // 3: D (below the shared face)
            [0.0, 0.0, 1.0],  // 4: E (above the shared face)
        ],
        tets: vec![[0, 1, 3, 2], [0, 1, 2, 4]],
        physical_groups: Default::default(),
    }
}

/// The unit cube with its center node 8 = (½,½,½) as the common apex of
/// the 12 boundary triangles (2 per cube face). The bottom plate
/// `{0,1,2,3}` is ground and the top plate `{4,5,6,7}` is an electrode, so
/// every boundary face is prescribed (all-Dirichlet) — the interesting
/// faces are the 18 **interior** ones `(8, edge)`, all of which contain
/// the free apex and are therefore evaluated.
///
/// With `φ = z` (0 V on the bottom, 1 V on the top, ½ at the apex) the
/// gradient is exactly `ẑ` on all 12 tets.
fn cube() -> TetMesh {
    TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [0.0, 1.0, 0.0], // 2
            [1.0, 1.0, 0.0], // 3
            [0.0, 0.0, 1.0], // 4
            [1.0, 0.0, 1.0], // 5
            [0.0, 1.0, 1.0], // 6
            [1.0, 1.0, 1.0], // 7
            [0.5, 0.5, 0.5], // 8: free interior apex
        ],
        tets: vec![
            [8, 0, 1, 3],
            [8, 0, 3, 2],
            [8, 2, 3, 7],
            [8, 2, 7, 6],
            [8, 1, 3, 7],
            [8, 1, 7, 5],
            [8, 0, 1, 5],
            [8, 0, 5, 4],
            [8, 4, 5, 7],
            [8, 4, 7, 6],
            [8, 0, 2, 6],
            [8, 0, 6, 4],
        ],
        physical_groups: Default::default(),
    }
}

fn electrode(name: &str, nodes: &[u32], voltage: f64) -> Electrode {
    Electrode {
        name: name.to_string(),
        nodes: nodes.to_vec(),
        voltage,
    }
}

/// AC 1 (test plan): a constant-field solution with compatible boundary
/// data has zero residual to roundoff. `φ = z` on the apex cube (½ V at
/// the center), ground plate at `z = 0`, electrode plate at `z = 1`:
/// every tet's gradient is exactly `ẑ`, so the 12 evaluated interior
/// faces have zero jump (one gradient, one ε), the all-Dirichlet boundary
/// faces are prescribed, and `ρ = 0`.
#[test]
fn uniform_gradient_with_compatible_data_has_zero_residual() {
    let mesh = cube();
    let phi = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5];
    let eps_r = vec![1.0; 12];
    let rho = vec![0.0; 12];
    let ground = [0_u32, 1, 2, 3];
    let electrodes = [electrode("top", &[4, 5, 6, 7], 1.0)];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground).unwrap();

    assert!(
        r.eta.abs() < 1e-20,
        "eta = {} should be exactly zero",
        r.eta
    );
    assert!(r.faces.iter().all(|f| f.jump == 0.0));
    assert!(r.eta_t2.iter().all(|&t| t >= 0.0));
    // The 18 interior apex faces (12 cube edges + 6 face diagonals, each
    // shared by exactly two tets, each containing the free apex node) are
    // all present and counted exactly once; every boundary face is
    // all-Dirichlet (prescribed) and skipped.
    assert_eq!(r.faces.iter().filter(|f| f.is_interior()).count(), 18);
    assert_eq!(r.faces.len(), 18);
}

/// AC 1 (test plan), two-material version: with `ε_r = [1, 2]` and
/// `φ_E = 0.5` the normal displacement `D·n̂ = −ε ∇φ·n̂` is continuous
/// across the shared face (`2ε₀·0.5 = 1ε₀·1`), so the interior jump is
/// zero despite the material contrast.
#[test]
fn two_material_interface_continuous_normal_flux_has_zero_jump() {
    let mesh = bipyramid();
    // φ = 0 on ABC, −1 at D, +0.5 at E ⇒ ∇φ|_{T⁻} = +ẑ, ∇φ|_{T⁺} = 0.5 ẑ.
    let phi = vec![0.0, 0.0, 0.0, -1.0, 0.5];
    let eps_r = vec![1.0, 2.0];
    let rho = vec![0.0; 2];
    let ground = [3_u32];
    let electrodes = [electrode("top", &[4], 0.5)];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground).unwrap();

    let interior: Vec<_> = r.faces.iter().filter(|f| f.is_interior()).collect();
    assert_eq!(interior.len(), 1, "exactly one interior face (ABC)");
    let abc = interior[0];
    assert_eq!(abc.nodes, [0, 1, 2]);
    assert_eq!(abc.minus, 0, "T⁻ is the lower-index tet");
    assert_eq!(abc.plus, Some(1), "T⁺ is the higher-index tet");
    assert_eq!(abc.jump, 0.0, "continuous normal displacement ⇒ zero jump");
    // Area ½·|AB×AC| = ½, face diameter = |BC| = √2.
    close(abc.area, 0.5, 1.0);
    close(abc.h, 2.0_f64.sqrt(), 1.0);
}

/// AC 2 + hand-computed fixture (test plan): isolate a known flux jump.
/// With `ε_r = [1, 2]` and `φ_E = +1` the jump is
/// `J_ABC = ε₀(2·1 − 1·1) = +ε₀` (positive per the documented
/// orientation), shared ½/½ as `½ h_F J² A_F = (√2/4)ε₀²` to each tet.
/// The counted natural faces add hand-computed fluxes (`∇φ|_{T⁻} = ẑ`,
/// `∇φ|_{T⁺} = ẑ`, `ε_r = [1, 2]`): `J_BCD = +ε₀/√3` on tet 0 and
/// `J_BCE = −2ε₀/√3` on tet 1.
#[test]
fn material_jump_value_sign_and_face_shares() {
    let mesh = bipyramid();
    let phi = vec![0.0, 0.0, 0.0, -1.0, 1.0];
    let eps_r = vec![1.0, 2.0];
    let rho = vec![0.0; 2];
    let ground = [3_u32];
    let electrodes = [electrode("top", &[4], 1.0)];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground).unwrap();

    // Interior jump: value and sign.
    let abc = r
        .faces
        .iter()
        .find(|f| f.nodes == [0, 1, 2])
        .expect("interior face ABC must be present");
    assert!(abc.is_interior());
    close(abc.jump, EPS_0, EPS_0);

    // Per-tet indicators: interior share (√2/4)ε₀² plus each tet's own
    // natural-face flux (√6/6)ε₀² on BCD, (2√6/3)ε₀² on BCE.
    let share = 2.0_f64.sqrt() / 4.0 * EPS_0 * EPS_0;
    let bcd = 6.0_f64.sqrt() / 6.0 * EPS_0 * EPS_0;
    let bce = 2.0 * 6.0_f64.sqrt() / 3.0 * EPS_0 * EPS_0;
    close(r.eta_t2[0], share + bcd, EPS_0 * EPS_0);
    close(r.eta_t2[1], share + bce, EPS_0 * EPS_0);

    // Global η is the root of the sum of the nonnegative indicators, and
    // (ρ = 0) the sum of the per-tet η² equals the sum of the face terms
    // h_F J² A_F — each interior face counted once (its ½/½ split is
    // internal bookkeeping), each natural face fully on its tet.
    close(r.eta * r.eta, r.eta_t2.iter().sum::<f64>(), EPS_0 * EPS_0);
    let face_part: f64 = r.faces.iter().map(|f| f.h * f.jump * f.jump * f.area).sum();
    close(r.eta * r.eta, face_part, EPS_0 * EPS_0);

    // Face term sanity: the shared face's h_F and area.
    close(
        abc.h * abc.jump * abc.jump * abc.area,
        2.0 * share,
        EPS_0 * EPS_0,
    );
}

/// AC 1: isolate a nonzero charge. On the apex cube with `φ = z` (zero
/// face terms everywhere, as in the first test) a uniform `ρ = 1` C/m³
/// leaves only cell terms: each of the 12 tets has `h_T = √2` (its
/// longest edge is a base diagonal) and `V_T = 1/12`, so
/// `η² = 12 · 2 · 1 · (1/12) = 2 ρ²`. The signed cell residual is the
/// prescribed charge itself.
#[test]
fn cell_term_isolated_on_uniform_mesh() {
    let mesh = cube();
    let phi = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5];
    let eps_r = vec![1.0; 12];
    let rho = vec![1.0; 12];
    let ground = [0_u32, 1, 2, 3];
    let electrodes = [electrode("top", &[4, 5, 6, 7], 1.0)];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground).unwrap();

    assert!(r.faces.iter().all(|f| f.jump == 0.0), "no face terms");
    close(r.eta * r.eta, 2.0 * rho[0] * rho[0], 2.0);
    assert_eq!(r.cell_rho, rho);
}

/// AC 2: natural-boundary flux and prescribed-face exclusion on a single
/// tet with ground `{A, B, C}` and `φ_D = 1` (⇒ `∇φ = ẑ`, `E = −ẑ`). Face
/// `ABC` is prescribed (all three nodes ground) and skipped; the three
/// slanted faces are natural: their outward normals are `−ŷ`, `−x̂` and
/// `(1,1,1)/√3`, giving `J = 0, 0, −ε₀/√3`, so
/// `η² = √2·(ε₀/√3)²·(√3/2) = (√6/6)ε₀²`.
#[test]
fn natural_boundary_flux_and_prescribed_face_skipped() {
    let mesh = TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        tets: vec![[0, 1, 2, 3]],
        physical_groups: Default::default(),
    };
    let phi = vec![0.0, 0.0, 0.0, 1.0];
    let eps_r = vec![1.0];
    let rho = vec![0.0];
    let ground = [0_u32, 1, 2];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &[], &ground).unwrap();

    assert_eq!(r.faces.len(), 3, "prescribed face ABC must be skipped");
    assert!(r.faces.iter().all(|f| !f.is_interior()));
    assert!(!r.faces.iter().any(|f| f.nodes == [0, 1, 2]));

    let bcd = r.faces.iter().find(|f| f.nodes == [1, 2, 3]).unwrap();
    close(bcd.jump, -EPS_0 / 3.0_f64.sqrt(), EPS_0);
    close(bcd.area, 3.0_f64.sqrt() / 2.0, 1.0);
    close(bcd.h, 2.0_f64.sqrt(), 1.0);

    close(
        r.eta * r.eta,
        6.0_f64.sqrt() / 6.0 * EPS_0 * EPS_0,
        EPS_0 * EPS_0,
    );
}

/// AC 2 addendum: the prescribed-face rule is "all three nodes Dirichlet".
/// Tagging the shared face's nodes as an internal conductor sheet
/// (electrode `{A, B, C}`, no ground) removes the interior face from the
/// residual even though it is interior to the mesh, while every slanted
/// face with a free apex — including those with **two** Dirichlet nodes —
/// is still counted.
#[test]
fn internal_sheet_face_excluded_but_two_of_three_still_counted() {
    let mesh = bipyramid();
    let phi = vec![0.0, 0.0, 0.0, -1.0, 1.0];
    let eps_r = vec![1.0, 2.0];
    let rho = vec![0.0; 2];

    // Baseline: apexes pinned, shared face free ⇒ interior face counted.
    let counted = estimate_electrostatic_p1(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &[electrode("top", &[4], 1.0)],
        &[3],
    )
    .unwrap();
    assert_eq!(counted.faces.iter().filter(|f| f.is_interior()).count(), 1);

    // Internal conductor sheet on the shared face: ABC is skipped, the
    // slanted faces (two sheet nodes + one free apex) are still counted.
    let sheet = estimate_electrostatic_p1(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &[electrode("sheet", &[0, 1, 2], 0.0)],
        &[],
    )
    .unwrap();
    assert_eq!(sheet.faces.iter().filter(|f| f.is_interior()).count(), 0);
    assert_eq!(sheet.faces.len(), 6, "all six slanted faces counted");
    assert!(!sheet.faces.iter().any(|f| f.nodes == [0, 1, 2]));
    assert!(sheet.faces.iter().any(|f| f.nodes == [0, 1, 3]));
    // The residual dropped by exactly the interior share.
    let share = 2.0_f64.sqrt() / 4.0 * EPS_0 * EPS_0;
    close(
        counted.eta * counted.eta - sheet.eta * sheet.eta,
        2.0 * share,
        EPS_0 * EPS_0,
    );
}

/// A face shared by **three** tets (non-manifold adjacency) is an error
/// in every build profile: the third tet must not silently overwrite the
/// second neighbor. Regression for the review's bipyramid-plus-apex
/// fixture, where the old `debug_assert!` panicked in debug but a
/// release build dropped tet 1's share of face ABC and reported
/// `J_ABC/ε₀ = 3` as `Ok`.
#[test]
fn non_manifold_face_is_an_error() {
    let mut mesh = bipyramid();
    mesh.nodes.push([0.0, 0.0, 2.0]); // 5: apex above E, sharing face ABC
    mesh.tets.push([0, 1, 2, 5]); // third tet incident to ABC
    let phi = vec![0.0, 0.0, 0.0, -1.0, 1.0, 2.0]; // φ = z
    let eps_r = vec![1.0, 2.0, 4.0];
    let rho = vec![0.0; 3];

    let r = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &[], &[]);
    assert!(
        matches!(r, Err(ElectrostaticP1Error::NonManifoldFace(_))),
        "a face incident to three tets must error in every build profile, got {r:?}"
    );
}

/// Shape mismatches against the mesh are errors, mirroring the
/// assembler's contract.
#[test]
fn length_mismatch_is_a_shape_error() {
    let mesh = bipyramid();
    let phi = vec![0.0; 5];
    let eps_r = vec![1.0; 2];
    let rho = vec![0.0; 2];

    let short_phi = estimate_electrostatic_p1(&mesh, &phi[..4], &eps_r, &rho, &[], &[]);
    assert!(matches!(
        short_phi,
        Err(ElectrostaticP1Error::ShapeMismatch(_))
    ));

    let short_eps = estimate_electrostatic_p1(&mesh, &phi, &eps_r[..1], &rho, &[], &[]);
    assert!(matches!(
        short_eps,
        Err(ElectrostaticP1Error::ShapeMismatch(_))
    ));

    let short_rho = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho[..1], &[], &[]);
    assert!(matches!(
        short_rho,
        Err(ElectrostaticP1Error::ShapeMismatch(_))
    ));
}
