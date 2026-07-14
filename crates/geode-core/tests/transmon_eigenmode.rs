//! Transmon + readout-resonator eigenmode reproduction with the
//! Josephson junction as a **lumped reactive shunt** (Epic #476 Phase B,
//! issue #492).
//!
//! The junction is modeled as a linear inductor `L` in parallel with a
//! junction capacitance `C` on the `lumped_element` surface group. Per
//! the derivation in [`geode_core::eigen::transmon`], the reactive Robin
//! substitution keeps the pencil REAL symmetric:
//!
//! ```text
//! (K + K_port) x = ω² (M + M_port) x,
//! K_port = (ℓ/(w·L̃)) S_Γ,   M_port = (C̃·ℓ/w) S_Γ,
//! ```
//!
//! so the existing real shift-invert Lanczos solves it directly.
//!
//! # Assembly-path bridging decision
//!
//! The pencil is REAL (PEC + real rotated-sapphire ε tensor + real
//! K_port/M_port). We assemble via the **sparse** full-tensor Nédélec
//! path
//! ([`assemble_global_nedelec_with_full_tensors_sparse`]) with
//! `nu_tensor = I` and `epsilon_tensor = TransmonFixture::epsilon_tensor_r()`
//! (imaginary part exactly zero — sapphire lossless here). We then take
//! the **real parts** of the `[nnz]` value vectors (asserting the
//! imaginary parts are ~0), build a real faer `SparseColMat<f64>`, add the
//! K_port/M_port surface triplets, reduce over the PEC interior mask, and
//! feed the real Lanczos. This avoids the 142 GB dense wall of the Phase-A
//! smoke path at the real mesh's 157k-DOF scale.
//!
//! # Test tiers
//!
//! - **CI-fast (`--lib`-companion here, run in debug):** unit K_port/M_port
//!   checks live in the `eigen::transmon` module; this file adds a
//!   small-synthetic-fixture end-to-end formulation test
//!   ([`synthetic_reactive_shunt_end_to_end`]) and the mode-ID / scaling
//!   tripwires on that synthetic fixture.
//! - **Release / `#[ignore]`:** the full 157k-DOF real-mesh eigensolve
//!   ([`real_transmon_eigenmodes_release`]) with the blog sanity band and
//!   the committed Palace oracle comparison.

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::transmon::{
    LumpedReactiveShunt, ModeReport, ReactiveElementNatural, TransmonPencil,
    frequency_hz_from_lambda, lambda_shift_for_frequency_hz,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TetMesh, TransmonFixture, cube_tet_mesh, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// The DeviceLayout transmon mesh is in **micrometres** (substrate
/// 4 mm ≈ 4000 mesh units; see the provenance file).
const M_PER_UNIT: f64 = 1e-6;

/// Read a `[nnz]` Burn value tensor to a host Vec<f64>.
fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// Assemble the REAL transmon pencil value vectors `(k_vals, m_vals)` via
/// the sparse full-tensor path, asserting the imaginary parts vanish.
///
/// `epsilon_tensor` is the per-tet real rotated-sapphire (or identity)
/// tensor; `nu_tensor` is the identity (μ_r = 1, lossless). Returns the
/// scatter map (owning the pattern) and the two real `[nnz]` value
/// vectors.
fn assemble_real_pencil(
    mesh: &TetMesh,
    tet_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_tensor: &[[[c64; 3]; 3]],
) -> (Vec<f64>, Vec<f64>) {
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu_tensor = vec![identity; mesh.n_tets()];

    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        tet_sign,
        scatter,
        epsilon_tensor,
        &nu_tensor,
    );

    let k_re = vals_to_host(sys.k_re_vals);
    let k_im = vals_to_host(sys.k_im_vals);
    let m_re = vals_to_host(sys.m_re_vals);
    let m_im = vals_to_host(sys.m_im_vals);

    let max_k_im = k_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let max_m_im = m_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let scale = k_re
        .iter()
        .chain(m_re.iter())
        .fold(0.0_f64, |a, &b| a.max(b.abs()))
        .max(1.0);
    assert!(
        max_k_im <= 1e-9 * scale,
        "K imaginary part not negligible: {max_k_im} (scale {scale}) — pencil not real"
    );
    assert!(
        max_m_im <= 1e-9 * scale,
        "M imaginary part not negligible: {max_m_im} (scale {scale}) — pencil not real"
    );

    (k_re, m_re)
}

// -------------------------------------------------------------------------
// CI-fast: small synthetic fixture end-to-end + tripwires.
// -------------------------------------------------------------------------

/// A small synthetic "transmon-like" fixture: a unit cube with PEC on its
/// outer boundary except the `z = 0` face, one triangle-patch of which is
/// the junction `lumped_element` surface. This is NOT the physical
/// transmon — it just exercises the full [`TransmonPencil`] driver end to
/// end at a size the dense-comparable Lanczos can chew in debug.
struct SyntheticFixture {
    mesh: TetMesh,
    tet_edge_idx: Vec<[u32; 6]>,
    tet_edge_sign: Vec<[i8; 6]>,
    edges: Vec<[u32; 2]>,
    junction_faces: Vec<[u32; 3]>,
    interior_mask: Vec<bool>,
    epsilon_tensor: Vec<[[c64; 3]; 3]>,
}

fn synthetic_fixture(n: usize) -> SyntheticFixture {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&mesh);

    // Junction patch: the whole z = 0 face. A large surface patch gives
    // the reactive shunt appreciable coupling to the cavity modes (a
    // single tiny triangle on a coarse cube barely participates), so the
    // synthetic mode-ID / scaling tripwires have a measurable signal.
    let junction_faces: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12))
        .collect();

    // PEC on the whole outer boundary EXCEPT the z = 0 face (so the
    // junction patch is a free interior-ish surface). Metal = all boundary
    // faces with any coordinate at ±0/1 except z = 0.
    let metal: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            // boundary face
            let on_bnd = |c: usize, val: f64| {
                f.iter()
                    .all(|&v| (mesh.nodes[v as usize][c] - val).abs() < 1e-12)
            };
            (on_bnd(2, 1.0)
                || on_bnd(0, 0.0)
                || on_bnd(0, 1.0)
                || on_bnd(1, 0.0)
                || on_bnd(1, 1.0))
                // exclude z = 0
                && !f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12)
        })
        .collect();
    let interior_mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);

    // Uniform ε = 4 (dielectric-filled cavity) as a real isotropic tensor.
    let eps_val = 4.0;
    let tens: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { eps_val } else { 0.0 }, 0.0))
    });
    let epsilon_tensor = vec![tens; mesh.n_tets()];

    SyntheticFixture {
        mesh,
        tet_edge_idx,
        tet_edge_sign,
        edges,
        junction_faces,
        interior_mask,
        epsilon_tensor,
    }
}

/// Solve the synthetic fixture with a given reactive shunt and shift.
fn solve_synthetic(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes(&pencil, sigma, n_modes, M_PER_UNIT)
        .expect("synthetic eigensolve")
}

/// End-to-end formulation smoke on the synthetic fixture: the real pencil
/// assembles, the reactive shunt is added, and the Lanczos returns finite
/// modes with participation in [0, 1]. The junction term must MOVE the
/// spectrum (a nonzero K_port perturbs the modes vs. the bare cavity).
#[test]
fn synthetic_reactive_shunt_end_to_end() {
    let fx = synthetic_fixture(3);
    // Shift near the cavity's first interior mode — the dielectric cube
    // fundamental is O(1) in these units; probe a modest σ above 0 to
    // clear the gradient nullspace.
    let sigma = 3.0;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let modes = solve_synthetic(&fx, element, 1.0, 1.0, sigma, 4);
    assert!(!modes.is_empty(), "no modes returned");
    for m in &modes {
        assert!(m.lambda.is_finite() && m.lambda > 0.0, "λ = {}", m.lambda);
        assert!(m.frequency_hz.is_finite() && m.frequency_hz >= 0.0);
        assert!(
            (0.0..=1.0).contains(&m.participation),
            "participation {} out of [0,1]",
            m.participation
        );
    }

    // Bare cavity (junction removed → K_port = 0, keep M_port) must give a
    // DIFFERENT lowest λ than the shunted one: the inductive stiffness
    // raised the mode. Same shift so the comparison is apples-to-apples.
    let bare = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: f64::INFINITY,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    let l_shunt = modes[0].lambda;
    let l_bare = bare[0].lambda;
    assert!(
        (l_shunt - l_bare).abs() / l_bare > 1e-6,
        "junction term did not perturb the spectrum: shunt λ={l_shunt}, bare λ={l_bare}"
    );
    // Participation of the shunted lowest mode should be strictly positive
    // (the junction stores some inductive energy).
    assert!(
        modes[0].participation > 0.0,
        "expected positive junction participation, got {}",
        modes[0].participation
    );
}

/// Tripwire (synthetic): doubling `L̃` LOWERS the inductive stiffness
/// `K_port ∝ 1/L̃`, so the mode with junction participation shifts DOWN
/// in λ (ω ∝ √λ). We assert the direction on the highest-participation
/// mode.
#[test]
fn tripwire_l_doubling_lowers_participating_mode() {
    let fx = synthetic_fixture(3);
    let sigma = 3.0;
    // Small L̃ → strong inductive stiffness → a mode with appreciable
    // junction participation to track under the L-doubling.
    let base = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 2.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    let doubled = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 4.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    // Identify the highest-participation mode in the base set and match it
    // by index (same shift, same ordering).
    let (idx, _) = base
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.participation.partial_cmp(&b.1.participation).unwrap())
        .unwrap();
    let p = base[idx].participation;
    eprintln!("L-doubling tripwire: participating mode idx={idx}, p={p:.4}");
    assert!(p > 1e-2, "no participating mode to test (max p = {p})");
    assert!(
        doubled[idx].lambda < base[idx].lambda * (1.0 - 1e-9),
        "doubling L̃ must lower the participating mode: base λ={}, doubled λ={} (p={p})",
        base[idx].lambda,
        doubled[idx].lambda
    );
}

/// Tripwire (synthetic): mode-ID participation gives an ORDERED spread —
/// distinct modes carry distinct junction-energy fractions, which is what
/// the qubit-vs-resonator labeling keys on. On the symmetric synthetic
/// cube (whole-face junction patch) every mode couples somewhat, so the
/// spread is modest (a few×); the SHARP qubit≫resonator separation is a
/// property of the physically-localized junction and is asserted on the
/// real fixture in [`real_transmon_eigenmodes_release`]. Here we only
/// require that participation is a non-degenerate discriminator.
#[test]
fn tripwire_mode_id_participation_separates() {
    let fx = synthetic_fixture(3);
    // Small L̃ → strong K_port → the junction-aligned mode carries more
    // inductive energy than the others.
    let modes = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 2.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        3.0,
        6,
    );
    let pmax = modes
        .iter()
        .map(|m| m.participation)
        .fold(0.0_f64, f64::max);
    let pmin = modes
        .iter()
        .map(|m| m.participation)
        .fold(1.0_f64, f64::min);
    eprintln!("mode-ID spread: pmax={pmax:.4}, pmin={pmin:.4}");
    assert!(
        pmax > 1.8 * pmin.max(1e-6),
        "participation did not separate modes: pmax={pmax}, pmin={pmin}"
    );
    // And participation is a real fraction in [0, 1].
    assert!(pmax <= 1.0 && pmin >= 0.0);
}

// -------------------------------------------------------------------------
// Release / #[ignore]: full real-mesh eigensolve + Palace oracle.
// -------------------------------------------------------------------------

/// The DeviceLayout junction values (issue #492 / mesh::transmon).
const JUNCTION_L_H: f64 = 14.860e-9;
const JUNCTION_C_F: f64 = 5.5e-15;

/// Blog band [4.14, 5.591] GHz (non-gating sanity, ±5%).
const BLOG_BAND_GHZ: [f64; 2] = [4.14, 5.591];

/// Palace oracle: lowest 6 eigenmode Re{f} (GHz) on the IDENTICAL mesh at
/// matched first order, from a real run committed under
/// `reference/fixtures/transmon_palace/results_p1/eig.csv` (Palace
/// changeset `fba6a5b`). See `benchmarks/transmon_eigen/results.toml`.
const PALACE_MODES_GHZ: [f64; 6] = [
    5.151335830348,
    15.46052107794,
    17.49010903536,
    18.69165792915,
    20.69755679425,
    26.08089940472,
];

/// Palace's junction LC mode (the one with appreciable EPR): the physical
/// Josephson-junction resonance `f_LC = 1/(2π√(LC)) ≈ 17.60 GHz`.
const PALACE_JUNCTION_MODE_GHZ: f64 = 17.49010903536;

/// The ≤1% same-mesh cross-validation bar.
const PALACE_BAR_PCT: f64 = 1.0;

/// Full real-mesh eigensolve, GATED against the committed Palace oracle.
///
/// The junction is `L = 14.860 nH ∥ C = 5.5 fF` on the `lumped_element`
/// patch (`ê = +Y`); PEC on metal + exterior; readout ports open
/// (lossless v1). The physical modes span ~5–26 GHz (the junction LC mode
/// is at `f_LC = 1/(2π√(LC)) ≈ 17.6 GHz`, NOT in the blog's default
/// [4.14, 5.591] GHz band — that band is the blog's *optimization start*,
/// far from these unoptimized default L/C values). The shift is placed at
/// 18 GHz to bracket the physical band.
///
/// **Spurious-mode note:** a junction-surface-localized mode near 3.45 GHz
/// appears below the physical band. geode-fem's real Lanczos lacks the
/// divergence-free projection Palace applies, so a gradient-nullspace-
/// adjacent mode leaks in. It is filtered here by matching each computed
/// mode to the committed Palace spectrum — the spurious mode has no Palace
/// counterpart and is excluded from the ≤1% gate. Removing it at the
/// source (a tree-cotree / div-free gauge on the eigen path) is a
/// documented follow-on.
///
/// Gated behind `--ignored` (release): sparse LU on 133k interior DOFs.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_eigenmodes_release --nocapture
/// ```
#[test]
#[ignore = "157k-DOF sparse shift-invert eigensolve — release benchmark only"]
fn real_transmon_eigenmodes_release() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    eprintln!(
        "transmon fixture: {} nodes, {} tets",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    // Target the physical band (junction LC ≈ 17.6 GHz + neighboring cavity
    // modes). Request enough modes that the shift-invert window reaches the
    // top Palace mode (26 GHz) past the spurious low mode + nullspace
    // leakage. Shift at 20 GHz centers the physical band.
    let modes = solve_real_fixture(&f, 20.0e9, 12);

    eprintln!("computed modes (sorted by λ):");
    for (i, m) in modes.iter().enumerate() {
        eprintln!(
            "  mode[{i}]: f = {:.4} GHz (λ = {:.4e}), participation p = {:.4}",
            m.frequency_ghz(),
            m.lambda,
            m.participation
        );
    }

    // ---- Gate: each Palace mode has a geode-fem counterpart within 1%. --
    let mut worst = 0.0_f64;
    for &pf in PALACE_MODES_GHZ.iter() {
        // Nearest geode-fem mode to this Palace mode.
        let (best, rel) = modes
            .iter()
            .map(|m| (m, (m.frequency_ghz() - pf).abs() / pf))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        eprintln!(
            "  Palace {pf:.4} GHz ↔ geode {:.4} GHz (p = {:.4}) → {:.4}% ({})",
            best.frequency_ghz(),
            best.participation,
            rel * 100.0,
            if rel * 100.0 <= PALACE_BAR_PCT {
                "WITHIN ≤1% bar"
            } else {
                "OUTSIDE bar"
            }
        );
        worst = worst.max(rel * 100.0);
        assert!(
            rel * 100.0 <= PALACE_BAR_PCT,
            "Palace mode {pf:.4} GHz has no geode-fem mode within {PALACE_BAR_PCT}% \
             (nearest {:.4} GHz, {:.4}%)",
            best.frequency_ghz(),
            rel * 100.0
        );
    }
    eprintln!(
        "Palace same-mesh cross-validation: worst-case per-mode Δ = {worst:.4}% (bar {PALACE_BAR_PCT}%)"
    );

    // ---- Mode ID: the junction LC mode has p ≈ 1; cavity modes p ≈ 0. ----
    let junction = modes
        .iter()
        .max_by(|a, b| a.participation.partial_cmp(&b.participation).unwrap())
        .unwrap();
    eprintln!(
        "mode-ID: junction LC mode f = {:.4} GHz, participation p = {:.4}",
        junction.frequency_ghz(),
        junction.participation
    );
    assert!(
        junction.participation > 0.5,
        "junction mode participation {} not dominant",
        junction.participation
    );
    // The high-participation mode IS Palace's junction LC mode (≤1%).
    let jrel =
        (junction.frequency_ghz() - PALACE_JUNCTION_MODE_GHZ).abs() / PALACE_JUNCTION_MODE_GHZ;
    assert!(
        jrel * 100.0 <= PALACE_BAR_PCT,
        "junction mode {:.4} GHz vs Palace {PALACE_JUNCTION_MODE_GHZ:.4} GHz = {:.4}% > {PALACE_BAR_PCT}%",
        junction.frequency_ghz(),
        jrel * 100.0
    );
    // Cavity (low-participation) modes vastly outnumber and separate from
    // the junction mode — the qubit-vs-resonator discriminator.
    let n_cavity = modes.iter().filter(|m| m.participation < 0.1).count();
    assert!(
        n_cavity >= 3,
        "expected several low-p cavity modes, got {n_cavity}"
    );

    // ---- Blog sanity band (non-gating). ----
    // The resonator is the lowest PHYSICAL mode — i.e. the lowest mode that
    // has a Palace counterpart (this excludes the spurious ~3.45 GHz mode,
    // which has none). Compared against the blog's [4.14, 5.591] GHz
    // optimization-START band (these default L/C are pre-optimization, so a
    // miss here is expected and non-gating).
    if let Some(res) = modes
        .iter()
        .filter(|m| {
            m.frequency_ghz() > 1.0
                && PALACE_MODES_GHZ
                    .iter()
                    .any(|&p| (m.frequency_ghz() - p).abs() / p <= PALACE_BAR_PCT / 100.0)
        })
        .min_by(|a, b| a.frequency_ghz().partial_cmp(&b.frequency_ghz()).unwrap())
    {
        let f_ghz = res.frequency_ghz();
        let in_band = BLOG_BAND_GHZ.iter().any(|&b| (f_ghz - b).abs() / b < 0.05);
        eprintln!(
            "blog-band check (non-gating): resonator f = {f_ghz:.4} GHz {} of [{}, {}] GHz",
            if in_band {
                "WITHIN ±5%"
            } else {
                "outside ±5%"
            },
            BLOG_BAND_GHZ[0],
            BLOG_BAND_GHZ[1]
        );
    }
}

/// Real-mesh tripwire: doubling the junction inductance `L` shifts the
/// junction LC mode DOWN by the √L scaling (`f ∝ 1/√(LC)`, so `2L` → factor
/// `1/√2 ≈ 0.707`), since that mode has participation p ≈ 1. This is the
/// physical Josephson-frequency dependence, measured directly.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored tripwire_real_junction_l_doubling --nocapture
/// ```
#[test]
#[ignore = "two 157k-DOF sparse eigensolves — release benchmark only"]
fn tripwire_real_junction_l_doubling() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");

    // Base and doubled-L junction LC modes (target the LC band).
    let base = solve_real_fixture_with_l(&f, JUNCTION_L_H, 18.0e9, 8);
    let doubled = solve_real_fixture_with_l(&f, 2.0 * JUNCTION_L_H, 14.0e9, 8);

    let jmode = |ms: &[ModeReport]| -> ModeReport {
        ms.iter()
            .max_by(|a, b| a.participation.partial_cmp(&b.participation).unwrap())
            .unwrap()
            .clone()
    };
    let jb = jmode(&base);
    let jd = jmode(&doubled);
    let ratio = jd.frequency_ghz() / jb.frequency_ghz();
    eprintln!(
        "L-doubling tripwire: junction mode {:.4} GHz (p={:.3}) → {:.4} GHz (p={:.3}); \
         ratio {:.4} (√L prediction 1/√2 = {:.4})",
        jb.frequency_ghz(),
        jb.participation,
        jd.frequency_ghz(),
        jd.participation,
        ratio,
        1.0 / 2.0_f64.sqrt()
    );
    // The mode must move DOWN, and toward the 1/√2 prediction (allow slack
    // for the finite step and the p slightly below 1).
    assert!(
        jd.frequency_ghz() < jb.frequency_ghz(),
        "doubling L must lower the junction mode"
    );
    assert!(
        (ratio - 1.0 / 2.0_f64.sqrt()).abs() < 0.05,
        "junction mode did not follow √L scaling: ratio {ratio}, want ≈ {:.4}",
        1.0 / 2.0_f64.sqrt()
    );
}

/// Full-mesh solve helper (shared by the release test and any future
/// operator harness), with the DeviceLayout junction values. `sigma_f_hz`
/// places the shift; returns `n_modes` modes sorted by λ.
fn solve_real_fixture(f: &TransmonFixture, sigma_f_hz: f64, n_modes: usize) -> Vec<ModeReport> {
    solve_real_fixture_with_l(f, JUNCTION_L_H, sigma_f_hz, n_modes)
}

/// As [`solve_real_fixture`] but with an explicit junction inductance (for
/// the L-scaling tripwire).
fn solve_real_fixture_with_l(
    f: &TransmonFixture,
    l_henry: f64,
    sigma_f_hz: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    // PEC on metal + exterior boundary.
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "PEC reduction: {} edges → {} interior DOFs",
        edges.len(),
        n_interior
    );

    // Real rotated-sapphire ε tensor (lossless).
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    eprintln!("assembling sparse real pencil (nnz = {})...", scatter.nnz());
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    // Junction reactive shunt on the lumped_element patch.
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(l_henry, JUNCTION_C_F, M_PER_UNIT);
    eprintln!(
        "junction geometry: ℓ = {:.4} μm, w = {:.4} μm; L̃ = {:.4e} μm, C̃ = {:.4e} μm",
        jport.length, jport.width, element.l_natural, element.c_natural
    );
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };

    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    eprintln!("shift σ = {sigma:.4e} (= k² at {} GHz)", sigma_f_hz / 1e9);
    geode_core::eigen::transmon::solve_transmon_eigenmodes(&pencil, sigma, n_modes, M_PER_UNIT)
        .expect("real transmon eigensolve")
}

/// Sanity: the frequency↔λ conversion places the junction natural-unit
/// values and the shift where the issue derivation says (a fast guard so
/// the release run's unit plumbing is regression-tested in CI).
#[test]
fn unit_plumbing_matches_issue_derivation() {
    let e = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    assert!((e.l_natural - 1.18253e4).abs() < 5.0, "L̃ = {}", e.l_natural);
    assert!((e.c_natural - 621.17).abs() < 0.5, "C̃ = {}", e.c_natural);
    // 4 GHz on a μm mesh → λ ≈ 7.0e-9 μm⁻².
    let lam = lambda_shift_for_frequency_hz(4.0e9, M_PER_UNIT);
    assert!((lam - 7.03e-9).abs() < 0.1e-9, "λ(4 GHz) = {lam}");
    assert!((frequency_hz_from_lambda(lam, M_PER_UNIT) - 4.0e9).abs() / 4.0e9 < 1e-12);
}
